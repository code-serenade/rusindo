use super::{router::Router, server::SocketEventSender};
use crate::{
    error::{Error, Result},
    websocket::events::SocketEvents,
    websocket::header_parser::HeaderParser,
};
use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::sync::Arc;
use tokio::{
    net::TcpStream,
    sync::{
        mpsc::{self, Receiver, Sender},
        oneshot,
    },
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

/// Alias for the writing half of a WebSocket connection.
type SocketWriter = SplitSink<WebSocketStream<TcpStream>, Message>;
/// Alias for the reading half of a WebSocket connection.
type SocketReader = SplitStream<WebSocketStream<TcpStream>>;
/// Type alias for a message containing an error code, command, and optional payload.
type Msg = (u16, u16, Option<BytesMut>);
/// Sender type alias for sending `Msg` between tasks.
pub type MsgSender = Sender<Msg>;
/// Receiver type alias for receiving `Msg` in a task.
type MsgReciver = Receiver<Msg>;

/// Represents a client connection.
#[derive(Debug)]
pub struct Connection {
    pub id: u32,
    pub msg_sender: MsgSender,
}

impl Connection {
    pub fn new(id: u32, msg_sender: MsgSender) -> Self {
        Self { id, msg_sender }
    }
}

pub async fn handle_connection(
    router: Arc<Router>,
    ws_stream: WebSocketStream<TcpStream>,
    sender: SocketEventSender,
    id: u32,
) {
    println!("socket id: {}", id);

    // Message channel
    let (msg_sender, msg_reciever) = mpsc::channel::<Msg>(4);

    let (tx, rx) = oneshot::channel::<u16>();

    let connection = Connection::new(id, msg_sender.clone());

    let conn_id = connection.id;

    // Send a handshake event to the connection manager
    sender
        .send(SocketEvents::Handshake(tx, connection))
        .unwrap();

    process_handshake(
        rx,
        ws_stream,
        router,
        msg_sender,
        sender,
        conn_id,
        msg_reciever,
    )
    .await
    .unwrap();
}

async fn process_handshake(
    rx: oneshot::Receiver<u16>,
    ws_stream: WebSocketStream<TcpStream>,
    router: Arc<Router>,
    msg_sender: MsgSender,
    socket_event_sender: SocketEventSender,
    connection_id: u32,
    msg_reciever: MsgReciver,
) -> Result<()> {
    match rx.await {
        Ok(error_code) => match error_code {
            0 => {
                let (socket_writer, socket_reader) = ws_stream.split();
                tokio::spawn(recieve_msg(msg_reciever, socket_writer));
                handle_msg(
                    router,
                    socket_reader,
                    msg_sender,
                    socket_event_sender,
                    connection_id,
                )
                .await
            }
            _ => Ok(()),
        },
        Err(_) => Ok(()),
    }
}

async fn recieve_msg(mut rx: MsgReciver, mut writer: SocketWriter) -> Result<()> {
    while let Some((error_code, cmd, response_data)) = rx.recv().await {
        let data = vec![error_code, cmd];
        let header = data.parse_header();

        let mut message = BytesMut::from(&header[..]);
        if let Some(data) = response_data {
            message.extend_from_slice(&data);
        }

        if let Err(e) = writer.send(Message::binary(message.freeze())).await {
            eprintln!("Error sending message: {}", e);
            return Err(Error::WsError(e));
        }
    }
    println!("recieve_msg task is exiting due to connection drop or other error.");
    Ok(())
}

async fn handle_msg(
    router: Arc<Router>,
    mut read: SocketReader,
    tx: MsgSender,
    socket_event_sender: SocketEventSender,
    connection_id: u32,
) -> Result<()> {
    while let Some(message) = read.next().await {
        let message = match message {
            // TODO 没有按照标准来实现，缺少处理ping、pong、close等消息
            Ok(msg) => {
                println!("received msg: {:?}", msg);
                msg
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
                return Err(Error::WsError(e));
            }
        };
        if message.is_binary() {
            let data = message.into_data();

            if data.len() >= 2 {
                // let deserialized: Vec<u16> = Vec::<u16>::deserialize_header(&data);
                // let cmd = deserialized[0];

                let cmd = BigEndian::read_u16(&data[0..2]);

                let payload = &data[2..];
                let message_data = BytesMut::from(payload);

                println!(
                    "Received message:  cmd={}, data={:?}",
                    cmd,
                    &message_data[..]
                );
                tokio::spawn(process_message(
                    cmd,
                    message_data,
                    router.clone(),
                    tx.clone(),
                    socket_event_sender.clone(),
                ));
            } else {
                eprintln!("Header too short: {}", data.len());
            }
        }
    }

    println!("WebSocket connection closed");
    socket_event_sender
        .send(SocketEvents::Disconnect(connection_id))
        .map_err(|e| Error::CustomError {
            message: format!("Failed to send disconnect event: {}", e),
            line: line!(),
            column: column!(),
        })?;

    Ok(())
}

async fn process_message(
    cmd: u16,
    message: BytesMut,
    router: Arc<Router>,
    tx: MsgSender,
    socket_event_sender: SocketEventSender,
) {
    match router
        .handle_message(cmd, message, socket_event_sender)
        .await
    {
        Ok(response_data) => {
            if let Err(e) = tx.send((0, cmd, Some(response_data))).await {
                eprintln!("Error sending processed message: {}", e);
            }
        }
        Err(_) => {
            let error_code = 1;
            if let Err(e) = tx.send((error_code, cmd, None)).await {
                eprintln!("Error sending processed message: {}", e);
            }
        }
    }
}
