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

async fn tcp_echo() {
    let socket = Socket::<IoUringInstanceType>::new();

    {
        let servaddr = libc::sockaddr_in {
            sin_family: libc::AF_INET as u16,
            sin_port: 3456_u16.to_be(),
            // s_addr should be htonl(INADDR_ANY)
            sin_addr: libc::in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };
        let ret = socket.bind(&servaddr);
        assert!(ret >= 0);
    }
    
    {
        let ret = socket.listen(10);
        assert_eq!(ret, 0);
    }
    println!("listen 127.0.0.1:3456");

    loop {
        if let Ok(client) = socket.accept(None).await {
            println!("accept");

            async_rt::task::spawn(async move {
                let mut buf = vec![0u8; 2048];

                loop {
                    println!("start read");
                    let bytes_read = client.read(buf.as_mut_slice()).await;

                    if bytes_read == 0 {
                        println!("shutdown");
                        break;
                    }

                    println!("start write");
                    let bytes_write = client.write(buf.as_slice()).await;

                    assert_eq!(bytes_read, bytes_write);

                    println!("read {}, write {}", bytes_read, bytes_write);
                }
            });
        } else {
            println!("accept() return err.");
        }
    }
}

fn main() {
    let ring = RING.clone();
    let actor = move || {
        ring.trigger_callbacks();
    };
    test_rt::register_actor(actor);
    test_rt::run_blocking(tcp_echo());
}
