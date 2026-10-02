use super::*;
fn block(values: &[f32]) -> Option<Block> {
    Some(
        Block::new(
            values.iter().flat_map(|v| [*v, -*v]).collect(),
            values.len(),
        )
        .unwrap(),
    )
}
#[test]
fn stable_output_following_handles_reorder_unplug_and_ambiguous_defaults() {
    use super::super::{devices::follow_stable_output, DeviceInfo};
    let device = |id: &str, default| DeviceInfo {
        id: format!("wasapi:{id}"),
        name: "Headphones".into(),
        direction: "output".into(),
        is_default: default,
    };
    let inventory = [device("b", false), device("a", true)];
    assert_eq!(
        follow_stable_output(&inventory, None).unwrap().id,
        "wasapi:a"
    );
    assert_eq!(
        follow_stable_output(&inventory, Some("wasapi:b"))
            .unwrap()
            .id,
        "wasapi:b"
    );
    assert_eq!(
        follow_stable_output(&inventory[1..], Some("wasapi:b"))
            .unwrap()
            .id,
        "wasapi:a"
    );
    assert!(follow_stable_output(&inventory[..1], None).is_err());
    assert!(follow_stable_output(&[device("a", true), device("b", true)], None).is_err());
    assert!(
        follow_stable_output(&[device("a", true), device("a", false)], Some("wasapi:a")).is_err()
    );
    assert!(follow_stable_output(&inventory, Some("output:0")).is_err());
    assert!(follow_stable_output(&inventory, Some("Headphones")).is_err());
    assert!(follow_stable_output(&[], None).is_err());
}

#[test]
fn unequal_blocks_keep_every_tail_frame_in_order() {
    let mut pending = [block(&[1.0, 2.0, 3.0, 4.0]), block(&[0.1, 0.2])];
    let mut time = Instant::now();
    let now = time;
    let mut output = Vec::new();
    assert_eq!(
        mix(&mut pending, &mut time, now, |v| output.push(v)).unwrap(),
        2
    );
    assert!(pending[0].is_some());
    assert!(pending[1].is_none());
    pending[1] = block(&[0.3, 0.4, 0.5]);
    assert_eq!(
        mix(&mut pending, &mut time, now, |v| output.push(v)).unwrap(),
        2
    );
    pending[0] = block(&[5.0]);
    assert_eq!(
        mix(&mut pending, &mut time, now, |v| output.push(v)).unwrap(),
        1
    );
    assert!(pending.iter().all(Option::is_none));
    for (frame, expected) in output.iter().zip([1.1_f32, 2.2, 3.3, 4.4, 5.5]) {
        assert!((frame[0] - expected).abs() < 1e-6);
        assert_eq!(frame[0], -frame[1]);
    }
}
#[test]
fn missing_stream_waits_boundedly_without_consuming_the_other_stream() {
    let mut pending = [block(&[0.2]), None];
    let mut time = Instant::now();
    let now = time;
    assert_eq!(
        mix(
            &mut pending,
            &mut time,
            now + Duration::from_millis(100),
            |_| panic!("unexpected frame")
        )
        .unwrap(),
        0
    );
    assert_eq!(pending[0].as_ref().unwrap().remaining(), 1);
    assert!(mix(
        &mut pending,
        &mut time,
        now + Duration::from_secs(3),
        |_| panic!("unexpected frame")
    )
    .is_err());
}
#[test]
fn digital_silence_is_valid_progress_and_nonfinite_one_source_does_not_erase_another() {
    let mut time = Instant::now();
    let now = time;
    let mut silence = [block(&[0.0]), block(&[0.0])];
    assert_eq!(
        mix(&mut silence, &mut time, now, |v| assert_eq!(v, [0.0; 2])).unwrap(),
        1
    );
    let mut malformed = [block(&[f32::NAN]), block(&[0.25])];
    mix(&mut malformed, &mut time, now, |v| {
        assert_eq!(v, [0.25, -0.25])
    })
    .unwrap();
}
#[test]
fn malformed_blocks_fail_before_indexing_or_output() {
    for (data, frames) in [
        (vec![], 0),
        (vec![0.0], 1),
        (vec![0.0; 4], 1),
        (vec![], usize::MAX),
    ] {
        assert!(Block::new(data, frames).is_err());
    }
}
#[test]
fn arbitrary_partitioning_matches_framewise_sum() {
    for split in 1..=31 {
        let mut pending = [None, None];
        let mut time = Instant::now();
        let now = time;
        let mut a = 0;
        let mut b = 0;
        let mut result = Vec::new();
        while result.len() < 97 {
            for (slot, cursor, size, gain) in
                [(0, &mut a, split, 0.01_f32), (1, &mut b, 32 - split, 0.02)]
            {
                if pending[slot].is_none() && *cursor < 97 {
                    let end = (*cursor + size).min(97);
                    pending[slot] =
                        block(&(*cursor..end).map(|i| i as f32 * gain).collect::<Vec<_>>());
                    *cursor = end;
                }
            }
            let count = mix(&mut pending, &mut time, now, |v| result.push(v)).unwrap();
            assert!(count > 0);
        }
        for (i, frame) in result.iter().enumerate() {
            assert!((frame[0] - i as f32 * 0.03).abs() < 1e-5);
        }
    }
}
