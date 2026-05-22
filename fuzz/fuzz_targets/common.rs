use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Runtime};

pub fn bounded(input: &[u8], max_len: usize) -> &[u8] {
    if input.len() > max_len {
        &input[..max_len]
    } else {
        input
    }
}

#[allow(dead_code)]
pub fn block_on<F>(future: F) -> F::Output
where
    F: Future,
{
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build fuzz runtime")
        })
        .block_on(future)
}
