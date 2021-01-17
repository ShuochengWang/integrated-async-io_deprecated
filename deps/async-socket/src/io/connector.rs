use std::sync::{Arc, Mutex};
use std::mem::ManuallyDrop;
use io_uring_callback::{Handle, Fd};

use crate::io::{Common, IoUringProvider};
use crate::poll::{Events, Pollee, Poller};

// TODO: import Handle from io_uring_callback crate
struct Handle;

pub struct Connector<P: IoUringProvider> {
    common: Arc<Common<P>>,
    // A pollee that is used by the connector privately. Not the one in common,
    // which is shared by all components (e.g., sender) of a socket.
    private_pollee: Pollee,
    inner: Mutex<Inner>,
}

struct Inner {
    pending_io: Option<Handle>,
    is_shutdown: bool,
    addr: ManuallyDrop<*mut libc::sockaddr_in>,
    addr_alloc: ManuallyDrop<libc::sockaddr_in>,
}

unsafe impl Send for Inner {}

impl<P: IoUringProvider> Connector<P> {
    pub(crate) fn new(common: Arc<Common<P>>) -> Self {
        let inner = Mutex::new(Inner::new());
        let private_pollee = Pollee::new(Events::empty());
        Self {
            common,
            private_pollee,
            inner,
        }
    }

    pub async fn connect(self: &Arc<Self>, addr: &libc::sockaddr_in) -> i32 {
        // Initiate the async connect
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.is_shutdown {
                return -libc::EPIPE;
            }

            // This method should be called once
            debug_assert!(inner.pending_io.is_some());

            unsafe { **inner.addr = *addr; }
            let handle = self.initiate_async_connect(*inner.addr as *const libc::sockaddr_in);
            inner.pending_io.replace(handle);
        }

        // Wait for the async connect to complete
        let mut poller = Poller::new();
        let events = self.private_pollee.poll_by(Events::IN, Some(&mut poller));
        if events.is_empty() {
            poller.wait().await;
        }

        // Finish the async connect
        {
            let inner = self.inner.lock().unwrap();
            let handle = inner.pending_io.as_ref().unwrap();
            handle.retval().unwrap()
        }
    }

    fn initiate_async_connect(self: &Arc<Self>, addr: *const libc::sockaddr_in) -> Handle {
        let connector = self.clone();
        let callback = move |retval: i32| {
            debug_assert!(retval <= 0);
            if retval == 0 {
                connector.private_pollee.add(Events::IN);
            } else {
                connector.private_pollee.add(Events::ERR);
            }
        };

        let io_uring = self.common.io_uring();
        let handle = unsafe {
            io_uring.connect(Fd(self.common.fd()), addr as *const libc::sockaddr, core::mem::size_of::<libc::sockaddr_in>() as u32, callback)
        };
        handle
    }

    pub fn is_shutdown(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.is_shutdown
    }

    pub fn shutdown(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.is_shutdown = true;
        drop(inner);

        // Wake up the blocking connect method
        self.private_pollee.add(Events::HUP);
    }
}

impl Inner {
    pub fn new() -> Self {
        let mut addr_alloc = unsafe { std::mem::zeroed() };
        let addr = &mut addr_alloc as *mut libc::sockaddr_in;
        Self {
            pending_io: None,
            is_shutdown: false,
            addr: ManuallyDrop::new(addr),
            addr_alloc: ManuallyDrop::new(addr_alloc),
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.addr);
            ManuallyDrop::drop(&mut self.addr_alloc);
        }
    }
}
