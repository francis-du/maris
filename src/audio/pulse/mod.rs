//! Linux system playback through the local PulseAudio protocol (also provided by PipeWire).
//! Application streams enter a temporary sink; only its monitor feeds Maris DSP.
mod route;
mod server;
mod stream;
use crate::{
    audio::{DeviceInfo, Metrics, Settings, Update},
    control::store::Store,
};
use anyhow::{ensure, Context, Result};
use crossbeam_queue::ArrayQueue;
use serde_json::{json, Value};
use server::{Server, Sink};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) struct Native {
    worker: Option<stream::Worker>,
    guard: route::Guard,
    sink: Sink,
    pub selected: Option<String>,
    checked: Instant,
    events: server::Events,
    graph_changed: bool,
}
impl Native {
    pub fn has_worker(&self) -> bool {
        self.worker.is_some()
    }
    pub fn healthy(&mut self) -> bool {
        self.worker.as_mut().is_some_and(stream::Worker::healthy)
    }
    pub fn pids(&self) -> Vec<i32> {
        self.guard.pids()
    }
    pub fn change_scope(&mut self, pids: &[i32]) -> Result<()> {
        self.guard.change_scope(pids)
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        self.worker.take();
        let _ = self.guard.restore();
    }
}

pub(super) struct ApplicationCapture {
    worker: stream::CaptureWorker,
    guard: route::Guard,
}
impl ApplicationCapture {
    pub fn prepare(
        store: &Store,
        pid: i32,
        queue: Arc<ArrayQueue<[f32; 2]>>,
        metrics: Arc<Metrics>,
    ) -> Result<Self> {
        let server = Server::discover()?;
        let guard = route::Guard::prepare(store, server.clone(), &[pid])?;
        let snapshot = server.snapshot()?;
        let private = snapshot
            .sinks
            .iter()
            .find(|sink| sink.name == guard.journal.sink)
            .context("Private application sink is unavailable")?;
        let worker = stream::CaptureWorker::start(
            &server,
            &private.monitor_source,
            &format!("{}_strip", guard.journal.token),
            queue,
            metrics,
        )?;
        Ok(Self { worker, guard })
    }
    pub fn start(&mut self) -> Result<()> {
        self.guard.reconcile()
    }
    pub fn check(&mut self) -> Result<()> {
        ensure!(
            self.guard.present(),
            "A captured application exited or changed identity"
        );
        ensure!(self.worker.healthy(), "Application capture stream failed");
        self.guard.reconcile()
    }
}

pub(super) fn devices() -> Result<Vec<DeviceInfo>> {
    let snapshot = Server::discover()?.snapshot()?;
    Ok(snapshot
        .sinks
        .iter()
        .filter(|sink| sink.physical())
        .map(|sink| DeviceInfo {
            id: format!("pulse:{}", sink.name),
            name: sink.label().to_owned(),
            direction: "output".into(),
            is_default: sink.name == snapshot.default,
        })
        .collect())
}
pub(super) fn applications() -> Result<Value> {
    let snapshot = Server::discover()?.snapshot()?;
    let applications: Vec<_> = snapshot.inputs.iter().filter_map(|input| {
        let pid = input.pid()?;
        Some(json!({"object_id":input.index,"pid":pid,"bundle_id":input.identity().and_then(|i|i.app),
            "display_name":input.app(),"running_output":true,"is_maris":input.own(),
            "devices":snapshot.sinks.iter().filter(|s|s.index==input.sink).map(|s|s.label()).collect::<Vec<_>>()}))
    }).collect();
    Ok(
        json!({"available":true,"backend":"pulse_server","applications":applications,
        "per_application_capture":true,"per_application_routing":true,"per_application_volume":false,
        "note":"Selected applications share one Maris DSP/output. Native PipeWire streams not exposed by the pulse server are not captured."}),
    )
}
pub(super) fn doctor() -> Value {
    match Server::discover()
        .and_then(|server| server.snapshot())
        .and_then(|snapshot| snapshot.output(None))
    {
        Ok(sink) => json!({"available":true,"backend":"pulse_server","default_output":sink.label(),
            "driver_required":false,"microphone_required":false,"capture_started":false}),
        Err(error) => {
            json!({"available":false,"backend":"pulse_server","reason":format!("{error:#}"),"capture_started":false})
        }
    }
}
pub(super) fn recover(store: &Store) -> Result<()> {
    route::recover(store, None)
}
pub(crate) fn watch(store: &Store, token: &str) -> Result<()> {
    route::watch(store, token)
}

mod session {
    use super::*;
    use crate::{
        analysis::AnalysisWorker,
        audio::{OutputBinding, Session},
        devices::identity as device_identity,
    };
    use std::sync::atomic::Ordering;

    struct Pipeline {
        worker: stream::Worker,
        metrics: Arc<Metrics>,
        updates: Arc<ArrayQueue<Update>>,
        analysis: AnalysisWorker,
        binding: OutputBinding,
        revision: u64,
        music_revision: u64,
        capability: crate::devices::capability::Capability,
    }
    fn pipeline(store: &Store, guard: &route::Guard, sink: &Sink) -> Result<Pipeline> {
        let resolution = device_identity::resolve_or_remember(
            store,
            device_identity::Identity {
                platform: "linux".into(),
                stable_id: Some(sink.name.clone()),
                model_id: None,
                display_name: sink.label().into(),
                source: "pulse_sink_name".into(),
            },
        )?;
        let state = store.load()?;
        let listening = crate::tuning::preferences::load(store)?;
        let capability = crate::devices::capability::effective(store, &resolution.profile_key)?;
        let target = crate::devices::capability::apply_constraints(
            listening.effective(&resolution.profile_key),
            &capability,
        );
        let baseline = Settings::compile(&state.profile, 48_000)?
            .with_music(&target.output_switch_baseline(), 48_000)?;
        let target = Settings::compile(&state.profile, 48_000)?
            .with_music(&target, 48_000)?
            .with_transition_ms(48_000, 120);
        let (baseline, target) = baseline.share_transition_headroom(target);
        let analysis = AnalysisWorker::start(48_000)?;
        let metrics = Arc::new(Metrics {
            analysis_dropped: analysis.dropped.clone(),
            ..Metrics::default()
        });
        let _ = metrics.analysis_queue.set(analysis.queue.clone());
        metrics.revision.store(state.revision, Ordering::Relaxed);
        metrics
            .music_revision
            .store(listening.revision, Ordering::Relaxed);
        let updates = Arc::new(ArrayQueue::new(8));
        ensure!(
            updates
                .push(Update {
                    settings: target,
                    revision: state.revision,
                    music_revision: listening.revision
                })
                .is_ok(),
            "Missing output update queue"
        );
        let worker = stream::Worker::start(
            &guard.server,
            &format!("{}.monitor", guard.journal.sink),
            &sink.name,
            &guard.journal.token,
            baseline,
            updates.clone(),
            metrics.clone(),
        )?;
        Ok(Pipeline {
            worker,
            metrics,
            updates,
            analysis,
            revision: state.revision,
            music_revision: listening.revision,
            capability,
            binding: OutputBinding {
                resolution,
                #[cfg(target_os = "macos")]
                coreaudio_id: None,
                buffer_frames: None,
                latency_frames: None,
                safety_offset_frames: None,
            },
        })
    }
    fn wait_playback(guard: &route::Guard, sink: &Sink) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = guard.server.snapshot()?;
            if snapshot.inputs.iter().any(|input| {
                input.own()
                    && input.sink == sink.index
                    && input.stream_name().as_deref() == Some(&guard.journal.token)
            }) {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "Maris playback did not become ready; applications remain on their original outputs");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    impl Session {
        pub(in crate::audio) fn native_pulse(
            store: Store,
            pids: &[i32],
            output: Option<&str>,
        ) -> Result<Self> {
            let lease = store.session_lock()?;
            recover(&store)?;
            let server = Server::discover()?;
            let sink = server.snapshot()?.output(output)?;
            let context = crate::analysis::context::Worker::start()?;
            let events = server.events()?;
            let mut guard = route::Guard::prepare(&store, server, pids)?;
            let p = pipeline(&store, &guard, &sink)?;
            wait_playback(&guard, &sink)?;
            guard.reconcile()?;
            p.worker.arm();
            Ok(Self {
                streams: Vec::new(),
                route: None,
                _lease: lease,
                updates: p.updates,
                metrics: p.metrics,
                store,
                input_name: "System playback (local pulse server)".into(),
                output_name: sink.label().into(),
                output_profile_key: p.binding.resolution.profile_key,
                output_identity: p.binding.resolution.identity,
                output_binding_source: p.binding.resolution.binding_source,
                output_binding_revision: p.binding.resolution.binding_revision,
                output_buffer_frames: None,
                output_latency_frames: None,
                output_safety_offset_frames: None,
                rebind_count: 0,
                last_rebind_ms: None,
                sample_rate: 48_000,
                sent_revision: p.revision,
                sent_music_revision: Some(p.music_revision),
                sent_capability: p.capability,
                last_status: Instant::now() - Duration::from_secs(1),
                completion: None,
                analysis: p.analysis,
                context,
                voice: None,
                session_id: crate::audio::new_session_id(),
                stop_requested: false,
                device_match_started: false,
                last_control_result: None,
                #[cfg(target_os = "macos")]
                tap: None,
                #[cfg(target_os = "macos")]
                system_output: None,
                #[cfg(target_os = "macos")]
                system_processes: Vec::new(),
                #[cfg(target_os = "macos")]
                system_process_pids: Vec::new(),
                #[cfg(target_os = "macos")]
                system_device_id: 0,
                #[cfg(target_os = "macos")]
                last_device_check: Instant::now(),
                pulse: Some(Native {
                    worker: Some(p.worker),
                    guard,
                    sink,
                    selected: output.map(str::to_owned),
                    checked: Instant::now(),
                    events,
                    graph_changed: false,
                }),
            })
        }
        pub(in crate::audio) fn rebind_pulse(&mut self, output: Option<&str>) -> Result<()> {
            let native = self
                .pulse
                .as_mut()
                .context("No active local-server audio session")?;
            let snapshot = native.guard.server.snapshot()?;
            let sink = snapshot.output(output)?;
            native.worker.take();
            let create = |sink: &Sink| -> Result<Pipeline> {
                let p = pipeline(&self.store, &native.guard, sink)?;
                wait_playback(&native.guard, sink)?;
                Ok(p)
            };
            let (p, sink, selection, failure) = match create(&sink) {
                Ok(p) => (p, sink, output.map(str::to_owned), None),
                Err(error) => {
                    let old = create(&native.sink).map_err(|restore| anyhow::anyhow!(
                        "Output switch failed: {error:#}; restoring the previous output failed: {restore:#}"))?;
                    (
                        old,
                        native.sink.clone(),
                        native.selected.clone(),
                        Some(error),
                    )
                }
            };
            p.worker.arm();
            native.worker = Some(p.worker);
            native.sink = sink;
            native.selected = selection;
            self.metrics = p.metrics;
            self.updates = p.updates;
            self.analysis = p.analysis;
            self.output_name = native.sink.label().into();
            self.output_profile_key = p.binding.resolution.profile_key;
            self.output_identity = p.binding.resolution.identity;
            self.output_binding_source = p.binding.resolution.binding_source;
            self.output_binding_revision = p.binding.resolution.binding_revision;
            self.sent_revision = p.revision;
            self.sent_music_revision = Some(p.music_revision);
            self.sent_capability = p.capability;
            self.rebind_count = self.rebind_count.saturating_add(1);
            self.last_rebind_ms = Some(crate::analysis::now_ms());
            self.device_match_started = false;
            self.context.reset();
            match failure {
                Some(error) => Err(error),
                None => Ok(()),
            }
        }
        pub(in crate::audio) fn rebind_pulse_processes(&mut self, pids: &[i32]) -> Result<()> {
            let native = self
                .pulse
                .as_mut()
                .context("No active local-server audio session")?;
            let previous = native.pids();
            let output = native.selected.clone();
            native.change_scope(pids)?;
            if let Err(error) = self.rebind_pulse(output.as_deref()) {
                if let Some(native) = self.pulse.as_mut() {
                    if native.change_scope(&previous).is_err() {
                        native.worker.take();
                    }
                }
                return Err(error);
            }
            Ok(())
        }
        pub(in crate::audio) fn refresh_pulse(&mut self) -> Result<()> {
            let Some(native) = self.pulse.as_mut() else {
                return Ok(());
            };
            let healthy = native.healthy();
            native.graph_changed |= native.events.changed()?;
            if healthy && !native.graph_changed && native.checked.elapsed() < Duration::from_secs(5)
            {
                return Ok(());
            }
            native.graph_changed = false;
            native.checked = Instant::now();
            let snapshot = native.guard.server.snapshot()?;
            let selected = native.selected.clone().filter(|_| {
                snapshot
                    .sinks
                    .iter()
                    .any(|s| s.name == native.sink.name && s.physical())
            });
            let target = snapshot.output(selected.as_deref())?;
            let change = target.name != native.sink.name;
            ensure!(
                healthy || change,
                "Local audio stream failed; restoring application outputs"
            );
            if change {
                self.rebind_pulse(selected.as_deref())?;
            }
            self.pulse
                .as_mut()
                .expect("active pulse session")
                .guard
                .reconcile_snapshot(&snapshot)
        }
    }
    use anyhow::Context;
}
