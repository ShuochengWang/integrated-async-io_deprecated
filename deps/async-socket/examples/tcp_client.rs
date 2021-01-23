use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use async_socket::{Socket, IoUringProvider};
use io_uring_callback::{Builder, IoUring};
use lazy_static::lazy_static;

mod test_rt;

lazy_static! {
    static ref RING: Arc<IoUring> = Arc::new(Builder::new().build(1024).unwrap());
}

struct IoUringInstanceType {}

impl IoUringProvider for IoUringInstanceType {
    type Instance = Arc<IoUring>;

    fn get_instance() -> Self::Instance {
        RING.clone()
    }
}

async fn tcp_client() {
    let socket = Socket::<IoUringInstanceType>::new();

    {
        let servaddr = libc::sockaddr_in {
            sin_family: libc::AF_INET as u16,
            sin_port: 3456_u16.to_be(),
            // s_addr should be htonl(INADDR_ANY)
            sin_addr: libc::in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };
        let ret = socket.connect(&servaddr).await;
        assert!(ret >= 0);
        println!("[client] connected!");
    }

    let mut buf = vec![0u8; 2048];
    let mut i: i32 = 0;
    loop {
        let bytes_write = socket.write(buf.as_slice()).await;

        let bytes_read = socket.read(buf.as_mut_slice()).await;

        assert_eq!(bytes_read, bytes_write);

        i += 1;
        if i > 0 {
            break;
        }
    }

    socket.shutdown(libc::SHUT_RDWR);
}

fn main() {
    let ring = RING.clone();
    let actor = move || {
        ring.trigger_callbacks();
    };
    test_rt::register_actor(actor);
    
    test_rt::run_blocking(tcp_client());
}
