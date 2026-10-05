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
