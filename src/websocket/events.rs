use super::connection::Connection;
// use bytes::BytesMut;
use tokio::sync::oneshot::Sender;

pub enum SocketEvents {
    Handshake(Sender<u16>, Connection),
    Disconnect(u32),
}
