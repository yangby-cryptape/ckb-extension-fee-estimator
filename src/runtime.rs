use std::sync::{atomic, Arc};

use tokio::runtime::Builder;
pub(crate) use tokio::runtime::Runtime as RawRuntime;

use crate::{
    arguments::Arguments,
    error::{Error, Result},
};

pub(crate) type Runtime = Arc<RawRuntime>;

pub(crate) fn initialize(_args: &Arguments) -> Result<Runtime> {
    Builder::new_multi_thread()
        .worker_threads(8)
        .max_blocking_threads(64)
        .enable_time()
        .enable_io()
        .thread_name_fn(|| {
            static ATOMIC_ID: atomic::AtomicUsize = atomic::AtomicUsize::new(0);
            let id = ATOMIC_ID.fetch_add(1, atomic::Ordering::SeqCst);
            format!("runtime-{}", id)
        })
        .build()
        .map_err(Error::runtime)
        .map(Arc::new)
}
