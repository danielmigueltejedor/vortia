//! Stopping a run from outside.
//!
//! A solver checks the flag at the top of each step, so a stop takes effect between steps and
//! never leaves a step half done. The flag belongs to the thread that runs the solver, so two
//! runs on two threads stop independently and a solver needs no extra argument.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Result, SolverError};

thread_local! {
    static FLAG: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Clears the flag of the thread when the run ends, even by a panic.
struct Armed;

impl Drop for Armed {
    fn drop(&mut self) {
        FLAG.with(|f| *f.borrow_mut() = None);
    }
}

/// Runs `run` with `flag` as this thread's stop flag: once it is set, the solver stops at the
/// start of its next step with [`SolverError::Cancelled`].
pub fn with_flag<T>(flag: Arc<AtomicBool>, run: impl FnOnce() -> Result<T>) -> Result<T> {
    FLAG.with(|f| *f.borrow_mut() = Some(flag));
    let _armed = Armed;
    run()
}

/// Called at the top of every step of a solver.
pub(crate) fn check() -> Result<()> {
    let stopped = FLAG.with(|f| {
        f.try_borrow()
            .is_ok_and(|f| f.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)))
    });
    if stopped {
        Err(SolverError::Cancelled)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_stops_a_thread_without_a_flag() {
        assert!(check().is_ok());
    }

    #[test]
    fn a_set_flag_stops_the_run_and_is_forgotten_after_it() {
        let flag = Arc::new(AtomicBool::new(false));
        let result = with_flag(flag.clone(), || {
            check()?;
            flag.store(true, Ordering::Relaxed);
            check()
        });
        assert!(matches!(result, Err(SolverError::Cancelled)));
        assert!(check().is_ok(), "the flag outlived its run");
    }

    #[test]
    fn a_flag_stops_only_its_own_thread() {
        let flag = Arc::new(AtomicBool::new(true));
        let other = std::thread::spawn(|| check().is_ok());
        assert!(matches!(
            with_flag(flag, check),
            Err(SolverError::Cancelled)
        ));
        assert!(other.join().unwrap());
    }
}
