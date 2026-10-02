//! Per-test-thread time control. Never compiled into production binaries.
use std::{cell::Cell, marker::PhantomData, rc::Rc};

thread_local! {
    static NOW: Cell<Option<u64>> = const { Cell::new(None) };
}

pub(crate) fn current() -> Option<u64> {
    NOW.with(Cell::get)
}

pub(crate) struct Clock {
    previous: Option<u64>,
    // Drop must restore the thread that installed this clock.
    _thread: PhantomData<Rc<()>>,
}
impl Clock {
    pub(crate) fn freeze() -> Self {
        let now = super::now_ms();
        Self {
            previous: NOW.with(|clock| clock.replace(Some(now))),
            _thread: PhantomData,
        }
    }
    pub(crate) fn advance(&self, milliseconds: u64) {
        NOW.with(|clock| {
            clock.set(Some(
                clock.get().unwrap().checked_add(milliseconds).unwrap(),
            ));
        });
    }
}
impl Drop for Clock {
    fn drop(&mut self) {
        NOW.with(|clock| clock.set(self.previous));
    }
}

#[test]
fn nested_clocks_restore_time_and_do_not_change_other_threads() {
    assert_eq!(current(), None);
    {
        let outer = Clock::freeze();
        let start = super::now_ms();
        outer.advance(10);
        {
            let inner = Clock::freeze();
            inner.advance(20);
            assert_eq!(super::now_ms(), start + 30);
            std::thread::spawn(|| assert_eq!(current(), None))
                .join()
                .unwrap();
        }
        assert_eq!(super::now_ms(), start + 10);
    }
    assert_eq!(current(), None);
}

#[test]
fn panicking_test_scope_restores_the_real_clock() {
    assert!(std::panic::catch_unwind(|| {
        let clock = Clock::freeze();
        clock.advance(10_000);
        panic!("fixture panic");
    })
    .is_err());
    assert_eq!(current(), None);
}
