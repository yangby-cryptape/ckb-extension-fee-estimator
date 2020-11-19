use std::thread;

use futures::pending;
use tokio::runtime::Builder;
pub(crate) use tokio::runtime::Handle as Runtime;

use crate::{
    arguments::Arguments,
    error::{Error, Result},
};

pub(crate) fn initialize(_args: &Arguments) -> Result<Runtime> {
    // TODO tokio 0.3.x doesn't have handle.block_on.
    // let runtime = Builder::new_multi_thread()
    //     .worker_threads(8)
    //     .thread_name_fn(|| {
    //         static ATOMIC_ID: atomic::AtomicUsize = atomic::AtomicUsize::new(0);
    //         let id = ATOMIC_ID.fetch_add(1, atomic::Ordering::SeqCst);
    //         format!("runtime-{}", id)
    //     })
    let mut runtime = Builder::new()
        .threaded_scheduler()
        .core_threads(8)
        .max_threads(64)
        .enable_time()
        .enable_io()
        .thread_name("Runtime")
        .build()
        .map_err(Error::runtime)?;
    let handle = runtime.handle().to_owned();
    thread::spawn(move || {
        runtime.block_on(async { pending!() });
    });
    Ok(handle)
}
