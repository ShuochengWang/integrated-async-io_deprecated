use async_rt::prelude::*;
use lazy_static::lazy_static;

pub(crate) fn run_blocking<T: Send + 'static>(
    future: impl Future<Output = T> + 'static + Send,
) -> T {
    TEST_RT.run_blocking(future)
}

pub(crate) fn register_actor(actor: impl Fn() + Send + 'static) {
    TEST_RT.register_actor(actor);
}

const TEST_PARALLELISM: u32 = 3;

lazy_static! {
    static ref TEST_RT: TestRt = TestRt::new(TEST_PARALLELISM);
}

struct TestRt {
    threads: Vec<std::thread::JoinHandle<()>>,
}

impl TestRt {
    pub fn new(parallelism: u32) -> Self {
        async_rt::executor::set_parallelism(parallelism).unwrap();

        let threads = (0..parallelism)
            .map(|_| std::thread::spawn(|| async_rt::executor::run_tasks()))
            .collect::<Vec<_>>();
        Self { threads }
    }

    pub fn register_actor(&self, actor: impl Fn() + Send + 'static) {
        async_rt::executor::register_actor(actor);
    }

    pub fn run_blocking<T: Send + 'static>(
        &self,
        future: impl Future<Output = T> + 'static + Send,
    ) -> T {
        async_rt::task::block_on(future)
    }
}

impl Drop for TestRt {
    fn drop(&mut self) {
        // Shutdown the executor and free the threads
        async_rt::executor::shutdown();

        for th in self.threads.drain(0..self.threads.len()) {
            th.join().unwrap();
        }
    }
}