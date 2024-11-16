use crate::error::Result;
use bytes::BytesMut;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use super::server::SocketEventSender;

pub trait Handler {
    fn call(
        &self,
        data: BytesMut,
        tx: SocketEventSender,
    ) -> Pin<Box<dyn Future<Output = Result<BytesMut>> + Send>>;
}

impl<F, Fut> Handler for F
where
    F: Fn(BytesMut, SocketEventSender) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<BytesMut>> + Send + 'static,
{
    fn call(
        &self,
        data: BytesMut,
        tx: SocketEventSender,
    ) -> Pin<Box<dyn Future<Output = Result<BytesMut>> + Send>> {
        Box::pin((self)(data, tx))
    }
}

pub struct Router {
    routes: HashMap<u16, Arc<dyn Handler + Send + Sync>>,
}

impl Router {
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    pub fn add_route<H>(&mut self, cmd: u16, handler: H) -> &mut Self
    where
        H: Handler + Send + Sync + 'static,
    {
        self.routes.insert(cmd, Arc::new(handler));
        self
    }

    pub async fn handle_message(
        &self,
        cmd: u16,
        data: BytesMut,
        tx: SocketEventSender,
    ) -> Result<BytesMut> {
        if let Some(handler) = self.routes.get(&cmd) {
            handler.call(data, tx).await
        } else {
            Err(crate::error::Error::ErrorCode(1))
        }
    }
}
