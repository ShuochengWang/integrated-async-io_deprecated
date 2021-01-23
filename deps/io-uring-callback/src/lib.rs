//! An IoUring with callback-based async I/O APIs.
#![cfg_attr(sgx, no_std)]

#[cfg(sgx)]
extern crate sgx_types;
#[cfg(sgx)]
#[macro_use]
extern crate sgx_tstd as std;
#[cfg(sgx)]
extern crate sgx_trts;

extern crate atomic;
extern crate io_uring;
#[macro_use]
extern crate lazy_static;
#[cfg(not(use_slab))]
extern crate sharded_slab;
#[cfg(use_slab)]
extern crate slab;

#[cfg(sgx)]
pub use sgx_trts::libc;
#[cfg(sgx)]
use std::prelude::v1::*;

use std::io;
use std::sync::Arc;
#[cfg(all(use_slab, not(sgx)))]
use std::sync::Mutex;
#[cfg(all(use_slab, sgx))]
use std::sync::SgxMutex as Mutex;

use io_uring::opcode::{self, types};
#[cfg(not(use_slab))]
use sharded_slab::Slab;
#[cfg(use_slab)]
use slab::Slab;

use crate::operation::Token;

mod operation;

pub use io_uring::opcode::types::{Fd, Fixed};

#[cfg(not(use_slab))]
lazy_static! {
    static ref TOKEN_SLAB: Slab<Token> = Slab::new();
}

#[cfg(use_slab)]
lazy_static! {
    static ref TOKEN_SLAB: Mutex<Slab<Token>> = Mutex::new(Slab::new());
}

pub struct IoUring {
    inner: Arc<io_uring::concurrent::IoUring>,
}

impl IoUring {
    pub(crate) fn new(inner: io_uring::IoUring) -> Self {
        let inner = Arc::new(inner.concurrent());
        Self { inner }
    }

    pub unsafe fn accept(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        addr: *mut libc::sockaddr,
        addrlen: *mut libc::socklen_t,
        flags: u32,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::Accept::new(fd, addr, addrlen)
            .flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn connect(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        addr: *const libc::sockaddr,
        addrlen: libc::socklen_t,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::Connect::new(fd, addr, addrlen)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn poll_add(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        flags: u32,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::PollAdd::new(fd, flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn poll_remove(
        &self,
        user_data: u64,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::PollRemove::new(user_data)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn read(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        buf: *mut u8,
        len: u32,
        offset: libc::off_t,
        flags: types::RwFlags,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::Read::new(fd, buf, len)
            .offset(offset)
            .rw_flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn write(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        buf: *const u8,
        len: u32,
        offset: libc::off_t,
        flags: types::RwFlags,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::Write::new(fd, buf, len)
            .offset(offset)
            .rw_flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn readv(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        iovec: *const libc::iovec,
        len: u32,
        offset: libc::off_t,
        flags: types::RwFlags,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::Readv::new(fd, iovec, len)
            .offset(offset)
            .rw_flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn writev(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        iovec: *const libc::iovec,
        len: u32,
        offset: libc::off_t,
        flags: types::RwFlags,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::Writev::new(fd, iovec, len)
            .offset(offset)
            .rw_flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn recvmsg(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        msg: *mut libc::msghdr,
        flags: u32,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::RecvMsg::new(fd, msg)
            .flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    pub unsafe fn sendmsg(
        &self,
        fd: Fd,
        // fixed_fd: Fixed,
        msg: *const libc::msghdr,
        flags: u32,
        callback: impl FnOnce(i32) + Send + 'static,
    ) -> Handle {
        let token_idx = self.gen_token(callback);

        let entry = opcode::SendMsg::new(fd, msg)
            .flags(flags)
            .build()
            .user_data(token_idx as _);
        if let Err(entry) = self.inner.submission().push(entry) {
            panic!("sq must be large enough");
        }
        if let Err(e) = self.inner.submit() {
            panic!("submit failed, error: {}", e);
        }

        let handle = self.gen_handle(token_idx);
        handle
    }

    /// Scan for completed async I/O and trigger their registered callbacks.
    #[cfg(not(use_slab))]
    pub fn trigger_callbacks(&self) {
        let cq = self.inner.completion();
        while let Some(cqe) = cq.pop() {
            let retval = cqe.result();
            let token_idx = cqe.user_data() as usize;
            let token = TOKEN_SLAB.get(token_idx).unwrap();
            let callback = token.complete(retval);
            (callback)(retval);
        }
    }

    #[cfg(use_slab)]
    pub fn trigger_callbacks(&self) {
        let cq = self.inner.completion();
        while let Some(cqe) = cq.pop() {
            let retval = cqe.result();
            let token_idx = cqe.user_data() as usize;
            let token_slab = TOKEN_SLAB.lock().unwrap();
            let token = token_slab.get(token_idx).unwrap();
            let callback = token.complete(retval);
            drop(token_slab);
            (callback)(retval);
        }
    }

    /// Cancel all ongoing async I/O.
    pub fn cancel_all(&self) {
        todo!();
    }

    #[cfg(not(use_slab))]
    fn gen_token(&self, callback: impl FnOnce(i32) + Send + 'static) -> usize {
        let token = Token::new(callback);
        let token_idx = TOKEN_SLAB.insert(token).unwrap();
        token_idx
    }

    #[cfg(use_slab)]
    fn gen_token(&self, callback: impl FnOnce(i32) + Send + 'static) -> usize {
        let token = Token::new(callback);
        let token_idx = TOKEN_SLAB.lock().unwrap().insert(token);
        token_idx
    }

    fn gen_handle(&self, token_idx: usize) -> Handle {
        Handle {
            io_uring: self.inner.clone(),
            token_idx,
        }
    }
}

pub struct Handle {
    io_uring: Arc<io_uring::concurrent::IoUring>,
    token_idx: usize,
}

impl Handle {
    #[cfg(not(use_slab))]
    pub fn retval(&self) -> Option<i32> {
        self.token().retval()
    }

    #[cfg(use_slab)]
    pub fn retval(&self) -> Option<i32> {
        TOKEN_SLAB.lock().unwrap().get(self.token_idx).unwrap().retval()
    }

    #[cfg(not(use_slab))]
    pub fn is_completed(&self) -> bool {
        self.token().is_completed()
    }

    #[cfg(use_slab)]
    pub fn is_completed(&self) -> bool {
        TOKEN_SLAB.lock().unwrap().get(self.token_idx).unwrap().is_completed()
    }

    pub fn cancel(&self) {
        todo!();
    }

    pub fn is_cancelled(&self) -> bool {
        false
    }

    pub fn user_data(&self) -> u64 {
        self.token_idx as _
    }

    #[cfg(not(use_slab))]
    fn token(&self) -> sharded_slab::Entry<Token> {
        TOKEN_SLAB.get(self.token_idx).unwrap()
    }
}

impl Drop for Handle {
    #[cfg(not(use_slab))]
    fn drop(&mut self) {
        let has_removed = TOKEN_SLAB.remove(self.token_idx);
        debug_assert!(has_removed);
    }

    #[cfg(use_slab)]
    fn drop(&mut self) {
        let mut token_slab = TOKEN_SLAB.lock().unwrap();
        debug_assert!(token_slab.contains(self.token_idx));
        token_slab.remove(self.token_idx);
    }
}

#[derive(Default)]
pub struct Builder {
    inner: io_uring::Builder,
}

impl Builder {
    pub fn new() -> Self {
        Default::default()
    }

    /// When this flag is specified, a kernel thread is created to perform submission queue polling.
    /// An io_uring instance configured in this way enables an application to issue I/O
    /// without ever context switching into the kernel.
    pub fn setup_sqpoll(&mut self, idle: impl Into<Option<u32>>) -> &mut Self {
        self.inner.setup_sqpoll(idle);
        self
    }

    /// If this flag is specified,
    /// then the poll thread will be bound to the cpu set in the value.
    /// This flag is only meaningful when [Builder::setup_sqpoll] is enabled.
    pub fn setup_sqpoll_cpu(&mut self, n: u32) -> &mut Self {
        self.inner.setup_sqpoll_cpu(n);
        self
    }

    /// Create the completion queue with struct `io_uring_params.cq_entries` entries.
    /// The value must be greater than entries, and may be rounded up to the next power-of-two.
    pub fn setup_cqsize(&mut self, n: u32) -> &mut Self {
        self.inner.setup_cqsize(n);
        self
    }

    /// Build a [IoUring].
    #[inline]
    pub fn build(&self, entries: u32) -> io::Result<IoUring> {
        let io_uring_inner = self.inner.build(entries)?;
        #[cfg(any(sgx, use_enter_thread))]
        io_uring_inner.start_enter_syscall_thread();
        let io_uring = IoUring::new(io_uring_inner);
        Ok(io_uring)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{IoSlice, IoSliceMut};
    use std::os::unix::io::AsRawFd;
    use std::sync::{Arc, Mutex};

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }

    #[test]
    fn test_builder() {
        let _io_uring = Builder::new().setup_sqpoll(1000).build(256).unwrap();
    }

    #[test]
    fn test_new() {
        let _io_uring = IoUring::new(io_uring::IoUring::new(256).unwrap());
    }

    #[test]
    fn test_writev_readv() {
        let io_uring = IoUring::new(io_uring::IoUring::new(256).unwrap());

        let fd = tempfile::tempfile().unwrap();
        let fd = Fd(fd.as_raw_fd());

        let text = b"1234";
        let text2 = b"5678";
        let mut output = vec![0; text.len()];
        let mut output2 = vec![0; text2.len()];

        let w_iovecs = vec![IoSlice::new(text), IoSlice::new(text2)];
        let r_iovecs = vec![IoSliceMut::new(&mut output), IoSliceMut::new(&mut output2)];

        let complete_io: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));

        let clone = complete_io.clone();
        let complete_fn = move |retval: i32| {
            let mut inner = clone.lock().unwrap();
            inner.replace(retval);
        };
        let handle = unsafe {
            io_uring.writev(
                fd,
                w_iovecs.as_ptr().cast(),
                w_iovecs.len() as _,
                0,
                0,
                complete_fn,
            )
        };
        loop {
            io_uring.trigger_callbacks();

            let clone = complete_io.clone();
            let mut inner = clone.lock().unwrap();
            if inner.is_some() {
                let retval = inner.take().unwrap();
                assert_eq!(retval, (text.len() + text2.len()) as i32);
                break;
            }
        }

        let clone = complete_io.clone();
        let complete_fn = move |retval: i32| {
            let mut inner = clone.lock().unwrap();
            inner.replace(retval);
        };
        let handle = unsafe {
            io_uring.readv(
                fd,
                r_iovecs.as_ptr().cast(),
                r_iovecs.len() as _,
                0,
                0,
                complete_fn,
            )
        };
        loop {
            io_uring.trigger_callbacks();

            let clone = complete_io.clone();
            let mut inner = clone.lock().unwrap();
            if inner.is_some() {
                let retval = inner.take().unwrap();
                assert_eq!(retval, (text.len() + text2.len()) as i32);
                assert_eq!(&output, text);
                assert_eq!(&output2, text2);
                break;
            }
        }
    }
}
