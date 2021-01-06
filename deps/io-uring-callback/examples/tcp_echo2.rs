use std::collections::VecDeque;
use std::net::TcpListener;
use std::os::unix::io::{AsRawFd, RawFd};
use std::ptr;
use std::sync::Mutex;

use io_uring::opcode::types;
use io_uring_callback::{Builder, Handle, IoUring};
use lazy_static::lazy_static;

lazy_static! {
    // (token_index, retval)
    static ref RES_QUEUE: Mutex<VecDeque<(usize, i32)>> = Mutex::new(VecDeque::new());
    static ref HANDLE_SLAB: sharded_slab::Slab<Handle> = sharded_slab::Slab::new();
}

#[derive(Clone, Debug)]
enum Token {
    Accept,
    Poll {
        fd: RawFd,
    },
    Read {
        fd: RawFd,
        buf_index: usize,
    },
    Write {
        fd: RawFd,
        buf_index: usize,
        offset: usize,
        len: usize,
    },
}

pub struct AcceptCount {
    fd: types::Fd,
    token_index: usize,
    count: usize,
}

impl AcceptCount {
    fn new(fd: RawFd, token_index: usize, count: usize) -> AcceptCount {
        AcceptCount {
            fd: types::Fd(fd),
            token_index,
            count,
        }
    }

    pub fn try_push_accept(&mut self, ring: &IoUring) {
        while self.count > 0 {
            let token_index = self.token_index;
            let slab_entry = HANDLE_SLAB.vacant_entry().unwrap();
            let slab_key = slab_entry.key();

            let complete_fn = move |retval: i32| {
                let mut queue = RES_QUEUE.lock().unwrap();
                queue.push_back((token_index, retval));

                HANDLE_SLAB.remove(slab_key);
            };

            let handle =
                unsafe { ring.accept(self.fd, ptr::null_mut(), ptr::null_mut(), 0, complete_fn) };

            slab_entry.insert(handle);

            self.count -= 1;
        }
    }
}

fn main() {
    let ring = Builder::new().build(256).unwrap();
    let listener = TcpListener::bind(("127.0.0.1", 3456)).unwrap();

    let mut bufpool = Vec::with_capacity(64);
    let mut buf_alloc = slab::Slab::with_capacity(64);
    let mut token_alloc = slab::Slab::with_capacity(64);

    println!("listen {}", listener.local_addr().unwrap());

    let mut accept = AcceptCount::new(listener.as_raw_fd(), token_alloc.insert(Token::Accept), 3);

    loop {
        accept.try_push_accept(&ring);

        ring.trigger_callbacks();

        let mut queue = RES_QUEUE.lock().unwrap();
        while !queue.is_empty() {
            let (token_index, ret) = queue.pop_front().unwrap();
            let token = &mut token_alloc[token_index];

            match token.clone() {
                Token::Accept => {
                    println!("accept");

                    accept.count += 1;

                    let fd = ret;
                    let poll_token_index = token_alloc.insert(Token::Poll { fd });

                    let slab_entry = HANDLE_SLAB.vacant_entry().unwrap();
                    let slab_key = slab_entry.key();

                    let complete_fn = move |retval: i32| {
                        let mut queue = RES_QUEUE.lock().unwrap();
                        queue.push_back((poll_token_index, retval));

                        HANDLE_SLAB.remove(slab_key);
                    };

                    let handle =
                        unsafe { ring.poll_add(types::Fd(fd), libc::POLLIN as _, complete_fn) };

                    slab_entry.insert(handle);
                }
                Token::Poll { fd } => {
                    let (buf_index, buf) = match bufpool.pop() {
                        Some(buf_index) => (buf_index, &mut buf_alloc[buf_index]),
                        None => {
                            let buf = vec![0u8; 2048].into_boxed_slice();
                            let buf_entry = buf_alloc.vacant_entry();
                            let buf_index = buf_entry.key();
                            (buf_index, buf_entry.insert(buf))
                        }
                    };

                    *token = Token::Read { fd, buf_index };
                    let slab_entry = HANDLE_SLAB.vacant_entry().unwrap();
                    let slab_key = slab_entry.key();

                    let complete_fn = move |retval: i32| {
                        let mut queue = RES_QUEUE.lock().unwrap();
                        queue.push_back((token_index, retval));

                        HANDLE_SLAB.remove(slab_key);
                    };

                    let handle = unsafe {
                        ring.read(
                            types::Fd(fd),
                            buf.as_mut_ptr(),
                            buf.len() as _,
                            0,
                            0,
                            complete_fn,
                        )
                    };

                    slab_entry.insert(handle);
                }
                Token::Read { fd, buf_index } => {
                    if ret == 0 {
                        bufpool.push(buf_index);

                        println!("shutdown");

                        unsafe {
                            libc::close(fd);
                        }
                    } else {
                        let len = ret as usize;
                        let buf = &buf_alloc[buf_index];

                        *token = Token::Write {
                            fd,
                            buf_index,
                            len,
                            offset: 0,
                        };

                        let slab_entry = HANDLE_SLAB.vacant_entry().unwrap();
                        let slab_key = slab_entry.key();

                        let complete_fn = move |retval: i32| {
                            let mut queue = RES_QUEUE.lock().unwrap();
                            queue.push_back((token_index, retval));

                            HANDLE_SLAB.remove(slab_key);
                        };

                        let handle = unsafe {
                            ring.write(types::Fd(fd), buf.as_ptr(), len as _, 0, 0, complete_fn)
                        };

                        slab_entry.insert(handle);
                    }
                }
                Token::Write {
                    fd,
                    buf_index,
                    offset,
                    len,
                } => {
                    let write_len = ret as usize;

                    if offset + write_len >= len {
                        bufpool.push(buf_index);

                        *token = Token::Poll { fd };

                        let slab_entry = HANDLE_SLAB.vacant_entry().unwrap();
                        let slab_key = slab_entry.key();

                        let complete_fn = move |retval: i32| {
                            let mut queue = RES_QUEUE.lock().unwrap();
                            queue.push_back((token_index, retval));

                            HANDLE_SLAB.remove(slab_key);
                        };

                        let handle =
                            unsafe { ring.poll_add(types::Fd(fd), libc::POLLIN as _, complete_fn) };

                        slab_entry.insert(handle);
                    } else {
                        let offset = offset + write_len;
                        let len = len - offset;

                        let buf = &buf_alloc[buf_index][offset..];

                        *token = Token::Write {
                            fd,
                            buf_index,
                            offset,
                            len,
                        };

                        let slab_entry = HANDLE_SLAB.vacant_entry().unwrap();
                        let slab_key = slab_entry.key();

                        let complete_fn = move |retval: i32| {
                            let mut queue = RES_QUEUE.lock().unwrap();
                            queue.push_back((token_index, retval));

                            HANDLE_SLAB.remove(slab_key);
                        };

                        let handle = unsafe {
                            ring.write(types::Fd(fd), buf.as_ptr(), len as _, 0, 0, complete_fn)
                        };

                        slab_entry.insert(handle);
                    };
                }
            }
        }
    }
}
