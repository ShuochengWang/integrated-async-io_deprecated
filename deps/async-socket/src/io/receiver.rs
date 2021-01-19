#[cfg(sgx)]
use sgx_trts::libc;
#[cfg(sgx)]
use std::prelude::v1::*;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::ptr::NonNull;
#[cfg(not(sgx))]
use std::sync::{Arc, Mutex, MutexGuard};
#[cfg(sgx)]
use std::sync::{Arc, SgxMutex as Mutex, SgxMutexGuard as MutexGuard};

use io_uring_callback::{Handle, Fd};
#[cfg(sgx)]
use untrusted_allocator::UntrustedAllocator;

use crate::io::{Common, IoUringProvider};
use crate::poll::{Events, Poller};
use crate::util::CircularBuf;


pub struct Receiver<P: IoUringProvider> {
    common: Arc<Common<P>>,
    inner: Mutex<Inner>,
}

struct Inner {
    buf: ManuallyDrop<CircularBuf>,
    #[cfg(not(sgx))]
    buf_alloc: ManuallyDrop<Vec<u8>>,
    #[cfg(sgx)]
    buf_alloc: ManuallyDrop<UntrustedAllocator>,
    pending_io: Option<Handle>,
    end_of_file: bool,
    iovecs: ManuallyDrop<*mut [MaybeUninit<libc::iovec>; 2]>,
    #[cfg(not(sgx))]
    iovecs_alloc: ManuallyDrop<[MaybeUninit<libc::iovec>; 2]>,
    #[cfg(sgx)]
    iovecs_alloc: ManuallyDrop<UntrustedAllocator>,
}

unsafe impl Send for Inner {}

impl<P: IoUringProvider> Receiver<P> {
    /// Construct the receiver of a socket.
    pub(crate) fn new(common: Arc<Common<P>>, buf_size: usize) -> Arc<Self> {
        let new_self = {
            let inner = Mutex::new(Inner::new(buf_size));
            Arc::new(Self { common, inner })
        };

        {
            let mut inner = new_self.inner.lock().unwrap();
            new_self.fill_buf(&mut inner);
        }

        new_self
    }

    pub async fn read(self: &Arc<Self>, buf: &mut [u8]) -> i32 {
        // Initialize the poller only when needed
        let mut poller = None;
        loop {
            // Attempt to read
            let ret = self.try_read(buf);
            if ret != -libc::EAGAIN {
                return ret;
            }

            // Wait for interesting events by polling
            if poller.is_none() {
                poller = Some(Poller::new());
            }
            let mask = Events::IN;
            let events = self.common.pollee().poll_by(mask, poller.as_mut());
            if events.is_empty() {
                poller.as_ref().unwrap().wait().await;
            }
        }
    }

    fn try_read(self: &Arc<Self>, buf: &mut [u8]) -> i32 {
        let mut inner = self.inner.lock().unwrap();

        if buf.len() == 0 {
            return 0;
        }

        let nbytes = inner.buf.consume(buf);

        if inner.buf.is_empty() {
            // Mark the socket as non-readable
            self.common.pollee().remove(Events::IN);
        }
        
        if inner.end_of_file {
            return nbytes as i32;
        }
        
        if nbytes == 0 {
            if let Some(error) = self.common.error() {
                return error;
            }
            return -libc::EAGAIN;
        }

        if inner.pending_io.is_none() {
            self.fill_buf(&mut inner);
        }
        nbytes as i32
    }

    fn fill_buf(self: &Arc<Self>, inner: &mut MutexGuard<Inner>) {
        debug_assert!(!inner.buf.is_full());
        debug_assert!(!inner.end_of_file);
        debug_assert!(inner.pending_io.is_none());

        // Init the callback invoked upon the completion of the async fill
        let receiver = self.clone();
        let complete_fn = move |retval: i32| {
            let mut inner = receiver.inner.lock().unwrap();

            // Release the handle to the async fill
            inner.pending_io.take();

            // Handle the two special cases of error and "end-of-file"
            if retval < 0 {
                receiver.common.set_error(retval);
                receiver.common.pollee().add(Events::ERR);
                return;
            } else if retval == 0 {
                inner.end_of_file = true;
                receiver.common.pollee().add(Events::IN);
                return;
            }

            // Handle the normal case of a successful read
            let nbytes = retval as usize;
            inner.buf.produce_without_copy(nbytes);

            // Attempt to fill again if there are free space in the buf.
            if !inner.buf.is_full() {
                receiver.fill_buf(&mut inner);
            }

            // Now that we have produced non-zero bytes, the buf must become
            // ready to read.
            receiver.common.pollee().add(Events::IN);
        };

        // Construct the iovec for the async fill
        let mut iovec_len = 1;
        let iovec_ptr = *inner.iovecs;
        unsafe {
            inner.buf.with_producer_view(|part0, part1| {
                debug_assert!(part0.len() > 0);
                (*iovec_ptr)[0].write(libc::iovec {
                    iov_base: part0.as_ptr() as _,
                    iov_len: part0.len() as _,
                });

                if part1.len() > 0 {
                    (*iovec_ptr)[1].write(libc::iovec {
                        iov_base: part1.as_ptr() as _,
                        iov_len: part1.len() as _,
                    });
                    iovec_len += 1;
                }

                // Only access the producer's buffer; zero bytes produced for now.
                0
            });
        }

        // Submit the async flush to io_uring
        let io_uring = &self.common.io_uring();
        let handle = unsafe {
            io_uring.readv(Fd(self.common.fd()), iovec_ptr as *const _, iovec_len, 0, 0, complete_fn)
        };
        inner.pending_io.replace(handle);
    }

    /// Shutdown the receiver.
    ///
    /// After shutdowning, the receiver will no longer be able to receive more data, except
    /// those that are already received in our or OS's receive buffer.
    pub fn shutdown(&self) {
        let inner = self.inner.lock().unwrap();
        unsafe {
            #[cfg(not(sgx))]
            libc::shutdown(self.common.fd(), libc::SHUT_RD);
            #[cfg(sgx)]
            libc::ocall::shutdown(self.common.fd(), libc::SHUT_RD);
        }
    }
}

impl Inner {
    pub fn new(buf_size: usize) -> Self {
        #[cfg(not(sgx))]
        let mut buf_alloc = Vec::<u8>::with_capacity(buf_size);
        #[cfg(sgx)]
        let buf_alloc = UntrustedAllocator::new(buf_size, 1).unwrap();

        let buf = unsafe {
            let ptr = NonNull::new_unchecked(buf_alloc.as_mut_ptr());
            let len = buf_alloc.capacity();
            CircularBuf::from_raw_parts(ptr, len)
        };
        let pending_io = None;
        let end_of_file = false;
        
        #[cfg(not(sgx))]
        let mut iovecs_alloc = unsafe { std::mem::zeroed() };
        #[cfg(not(sgx))]
        let iovecs = &mut iovecs_alloc as *mut [MaybeUninit<libc::iovec>; 2];
        
        #[cfg(sgx)]
        let iovecs_alloc = UntrustedAllocator::new(core::mem::size_of::<[MaybeUninit<libc::iovec>; 2]>(), 8).unwrap();
        #[cfg(sgx)]
        let iovecs = iovecs_alloc.as_mut_ptr() as *mut [MaybeUninit<libc::iovec>; 2];

        Inner {
            buf: ManuallyDrop::new(buf),
            buf_alloc: ManuallyDrop::new(buf_alloc),
            pending_io,
            end_of_file,
            iovecs: ManuallyDrop::new(iovecs),
            iovecs_alloc: ManuallyDrop::new(iovecs_alloc),
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // When the receiver is dropped, all pending async I/O should have been completed.
        debug_assert!(self.pending_io.is_none());

        // Since buf uses the memory allocated from buf_alloc, we must first drop buf,
        // then buf_alloc.
        unsafe {
            ManuallyDrop::drop(&mut self.buf);
            ManuallyDrop::drop(&mut self.buf_alloc);
            ManuallyDrop::drop(&mut self.iovecs);
            ManuallyDrop::drop(&mut self.iovecs_alloc);
        }
    }
}
