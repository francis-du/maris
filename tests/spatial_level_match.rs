use maris::{
    dsp::{Processor, Settings},
    music::MusicProfile,
    profile::Profile,
};

fn mid_side_gain_db(music: &MusicProfile) -> (f64, f64) {
    let mut processor = Processor::new(
        Settings::compile(&Profile::default(), 48_000)
            .unwrap()
            .with_music(music, 48_000)
            .unwrap(),
    );
    let mut mid_in = 0.0_f64;
    let mut mid_out = 0.0_f64;
    let mut side_in = 0.0_f64;
    let mut side_out = 0.0_f64;
    for i in 0..480_000 {
        let t = i as f64 / 48_000.0;
        let mid = 0.08 * (std::f64::consts::TAU * 997.0 * t).sin();
        let side = 0.05 * (std::f64::consts::TAU * 2_300.0 * t + 0.4).sin();
        let input = [(mid + side) as f32, (mid - side) as f32];
        let output = processor.process(input);
        if i >= 384_000 {
            let out_mid = (f64::from(output[0]) + f64::from(output[1])) * 0.5;
            let out_side = (f64::from(output[0]) - f64::from(output[1])) * 0.5;
            mid_in += mid * mid;
            mid_out += out_mid * out_mid;
            side_in += side * side;
            side_out += out_side * out_side;
        }
    }
    (
        10.0 * (mid_out / mid_in).log10(),
        10.0 * (side_out / side_in).log10(),
    )
}

#[test]
fn level_match_preserves_stereo_focus_relative_transfer() {
    let base = MusicProfile {
        level_match: true,
        ..MusicProfile::default()
    };
    let mut focus = base.clone();
    focus.stereo_focus = 1.0;

    let (base_mid, base_side) = mid_side_gain_db(&base);
    let (focus_mid, focus_side) = mid_side_gain_db(&focus);
    let relative_change = (focus_side - focus_mid) - (base_side - base_mid);
    let expected = 20.0 * 0.25_f64.log10();
    assert!(
        (relative_change - expected).abs() < 0.05,
        "Level Match changed Stereo Focus transfer: got {relative_change:.3} dB expected {expected:.3} dB"
    );
}

#[test]
fn level_match_preserves_stereo_width_relative_transfer() {
    let base = MusicProfile {
        level_match: true,
        ..MusicProfile::default()
    };
    let mut wide = base.clone();
    wide.width = 1.5;

    let (base_mid, base_side) = mid_side_gain_db(&base);
    let (wide_mid, wide_side) = mid_side_gain_db(&wide);
    let relative_change = (wide_side - wide_mid) - (base_side - base_mid);
    let expected = 20.0 * 1.5_f64.log10();
    assert!(
        (relative_change - expected).abs() < 0.05,
        "Level Match changed Width transfer: got {relative_change:.3} dB expected {expected:.3} dB"
    );
}
