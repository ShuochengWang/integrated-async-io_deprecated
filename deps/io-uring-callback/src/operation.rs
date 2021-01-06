//! Types that describe Async I/O operations.
use atomic::{Atomic, Ordering};
#[cfg(sgx)]
use std::prelude::v1::*;
#[cfg(not(sgx))]
use std::sync::Mutex;
#[cfg(sgx)]
use std::sync::SgxMutex as Mutex;

pub struct Token {
    state: Atomic<State>,
    callback: Mutex<Option<Box<dyn FnOnce(i32) + Send + 'static>>>,
}

impl Token {
    pub fn new(callback: impl FnOnce(i32) + Send + 'static) -> Self {
        let state = Atomic::new(State::Submitted);
        let callback = Mutex::new(Some(Box::new(callback) as _));
        Self { state, callback }
    }

    pub fn complete(&self, retval: i32) -> Box<dyn FnOnce(i32) + 'static> {
        loop {
            let old_state = self.state.load(Ordering::Acquire);
            debug_assert!(old_state == State::Submitted);
            let new_state = State::Completed(retval);
            if self
                .state
                .compare_exchange(old_state, new_state, Ordering::Release, Ordering::Relaxed)
                .is_ok()
            {
                return self.take_callback();
            }
        }
    }

    pub fn cancel(&self) {
        todo!();
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Cancelled
    }

    pub fn is_cancalling(&self) -> bool {
        self.state.load(Ordering::Acquire) == State::Cancelling
    }

    pub fn is_completed(&self) -> bool {
        self.retval().is_some()
    }

    pub fn retval(&self) -> Option<i32> {
        match self.state.load(Ordering::Acquire) {
            State::Completed(retval) => Some(retval),
            _ => None,
        }
    }

    fn take_callback(&self) -> Box<dyn FnOnce(i32) + 'static> {
        let mut callback_opt = self.callback.lock().unwrap();
        callback_opt.take().unwrap()
    }
}

impl Drop for Token {
    fn drop(&mut self) {
        debug_assert!(self.is_completed() || self.is_cancelled());
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum State {
    Submitted,
    Completed(i32),
    Cancelling,
    Cancelled,
}
