use crossbeam_queue::ArrayQueue;
use maris::resample::{ClockBridge, TAPS};
use std::sync::Arc;

#[test]
fn ready_means_reserve_filled_and_resets_after_starvation() {
    let queue = Arc::new(ArrayQueue::new(4096));
    let mut bridge = ClockBridge::new(queue.clone(), 480);
    assert!(!bridge.is_ready());
    for _ in 0..480 + TAPS - 1 {
        queue.push([0.0; 2]).unwrap();
    }
    assert_eq!(bridge.next_frame(), ([0.0; 2], false));
    assert!(!bridge.is_ready());
    queue.push([0.0; 2]).unwrap();
    assert_eq!(bridge.next_frame(), ([0.0; 2], false));
    assert!(bridge.is_ready());
    for _ in 0..4096 {
        bridge.next_frame();
    }
    assert!(!bridge.is_ready());
}
