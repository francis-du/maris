//! CPU neural enhancement runs on a worker, never inside a CPAL callback.
use anyhow::Result;
use crossbeam_queue::ArrayQueue;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

pub struct VoiceWorker {
    pub output: Arc<ArrayQueue<[f32; 2]>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    vad: Arc<AtomicU32>,
    blocks: Arc<AtomicU64>,
    drops: Arc<AtomicU64>,
    thread: Option<JoinHandle<()>>,
}
impl VoiceWorker {
    #[cfg(not(feature = "neural"))]
    pub fn start(_input: Arc<ArrayQueue<[f32; 2]>>, _rate: u32) -> Result<Self> {
        anyhow::bail!("RNNoise is not compiled in; rebuild with --features neural");
    }
    #[cfg(feature = "neural")]
    pub fn start(input: Arc<ArrayQueue<[f32; 2]>>, rate: u32) -> Result<Self> {
        anyhow::ensure!(
            rate == 48000,
            "Neural voice processing requires 48 kHz endpoints"
        );
        let output = Arc::new(ArrayQueue::new(12000));
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let vad = Arc::new(AtomicU32::new(0));
        let blocks = Arc::new(AtomicU64::new(0));
        let drops = Arc::new(AtomicU64::new(0));
        let (out, exit, running, probability, count, lost) = (
            output.clone(),
            stop.clone(),
            alive.clone(),
            vad.clone(),
            blocks.clone(),
            drops.clone(),
        );
        let thread = std::thread::Builder::new()
            .name("maris-rnnoise".into())
            .spawn(move || {
                struct Alive(Arc<AtomicBool>);
                impl Drop for Alive {
                    fn drop(&mut self) {
                        self.0.store(false, Ordering::Release);
                    }
                }
                let _alive = Alive(running);
                let mut model = crate::models::VoiceModel::new();
                let mut block = [[0.0_f32; 2]; 480];
                let mut processed = block;
                while !exit.load(Ordering::Relaxed) {
                    if input.len() < 480 {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                        continue;
                    }
                    for frame in &mut block {
                        *frame = input.pop().unwrap_or([0.0; 2]);
                    }
                    let voice = model.process(&block, &mut processed);
                    probability.store(voice.to_bits(), Ordering::Relaxed);
                    count.fetch_add(1, Ordering::Relaxed);
                    for frame in processed {
                        if out.push(frame).is_err() {
                            lost.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            })?;
        Ok(Self {
            output,
            stop,
            alive,
            vad,
            blocks,
            drops,
            thread: Some(thread),
        })
    }
    pub fn healthy(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }
    pub fn status(&self) -> Value {
        json!({"model":"rnnoise","local":true,"worker_alive":self.healthy(),"voice_probability":f32::from_bits(self.vad.load(Ordering::Relaxed)),"processed_blocks":self.blocks.load(Ordering::Relaxed),"dropped_frames":self.drops.load(Ordering::Relaxed)})
    }
}
impl Drop for VoiceWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
