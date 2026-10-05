use super::*;

#[test]
fn renderer_reports_actual_limiter_reduction_not_only_the_ceiling() {
    let metrics = Arc::new(Metrics::default());
    let mut renderer = Renderer::new(
        Settings::compile(&crate::profile::Profile::default(), 48_000).unwrap(),
        48_000,
        Arc::new(ArrayQueue::new(8)),
        metrics.clone(),
    );

    let mut warmup = [0.0_f32; 4_800];
    renderer.render(&mut warmup, 2, || ([0.1, -0.1], true));
    let idle = f32::from_bits(metrics.limiter_reduction.load(Ordering::Relaxed));
    assert!(
        idle.abs() < 0.01,
        "quiet program falsely reported limiter attenuation: {idle:.3} dB"
    );

    let mut loud = [0.0_f32; 1_024];
    renderer.render(&mut loud, 2, || ([2.0, -2.0], true));
    let reduction = f32::from_bits(metrics.limiter_reduction.load(Ordering::Relaxed));
    assert!(
        reduction > 5.0,
        "hard limiting was not visible in runtime telemetry: {reduction:.3} dB"
    );
    assert!(loud.iter().all(|sample| sample.abs() <= 0.891252));
}


#[test]
fn limiter_release_time_is_sample_rate_invariant() {
    let mut observed_ms = Vec::new();
    for rate in [44_100_u32, 48_000, 96_000, 192_000] {
        let settings = Settings::compile(&crate::profile::Profile::default(), rate).unwrap();
        let mut processor = crate::dsp::Processor::new(settings);
        let _ = processor.process([2.0, -2.0]);
        assert!(processor.limiter_reduction_db() > 5.0);

        let mut frames = 0_u32;
        while processor.limiter_reduction_db() > 0.1 && frames < rate {
            let _ = processor.process([0.0, 0.0]);
            frames += 1;
        }
        assert!(frames < rate, "limiter did not release at {rate} Hz");
        observed_ms.push(frames as f64 * 1000.0 / f64::from(rate));
    }
    let min = observed_ms.iter().copied().fold(f64::INFINITY, f64::min);
    let max = observed_ms.iter().copied().fold(0.0_f64, f64::max);
    assert!(
        max - min < 0.1,
        "limiter release changed with sample rate: {observed_ms:?}"
    );
}

