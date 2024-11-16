use super::connection::Connection;
use bytes::BytesMut;
// use bytes::BytesMut;
use tokio::sync::oneshot::Sender;

pub enum SocketEvents {
    Handshake(Sender<u16>, Connection),
    Disconnect(u32),
    Broadcast {
        error_code: u16,
        cmd: u16,
        message: Option<BytesMut>,
        connection_ids: Option<Vec<u32>>,
    },
}
