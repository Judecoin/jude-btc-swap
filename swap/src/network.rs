pub mod peer_tracker;
pub mod request_response;
pub mod transport;

use futures::prelude::*;
use libp2p::core::Executor;
use std::pin::Pin;
use tokio::runtime::Handle;

#[allow(missing_debug_implementations)]
pub struct TokioExecutor {
    pub handle: Handle,
}

impl Executor for TokioExecutor {
    fn exec(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        let _ = self.handle.spawn(future);
    }
}
