use std::sync::Arc;

use byteorder::{BigEndian, ByteOrder};
use bytes::BytesMut;
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use tokio::{
    net::TcpStream,
    sync::mpsc::{self, Receiver, Sender, UnboundedSender},
};
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};

use crate::{
    error::{Error, Result},
    websocket::events::SocketEvents,
    websocket::header_parser::HeaderParser,
};

use super::router::Router;

/// Alias for the writing half of a WebSocket connection.
type SocketWriter = SplitSink<WebSocketStream<TcpStream>, Message>;
/// Alias for the reading half of a WebSocket connection.
type SocketReader = SplitStream<WebSocketStream<TcpStream>>;
/// Type alias for a message containing an error code, command, and optional payload.
type Msg = (u16, u16, Option<BytesMut>);
/// Sender type alias for sending `Msg` between tasks.
type MsgSender = Sender<Msg>;
/// Receiver type alias for receiving `Msg` in a task.
type MsgReciver = Receiver<Msg>;

/// Represents a client connection.
#[derive(Debug)]
pub struct Connection {
    pub id: u32,
    pub name: String,
    pub msg_sender: MsgSender,
}

impl Connection {
    /// Creates a new `Connection` with the specified name and message sender.
    ///
    /// # Arguments
    ///
    /// * `name` - A `String` representing the connection's name.
    /// * `msg_sender` - A `MsgSender` used to send messages to this connection.
    ///
    /// # Returns
    ///
    /// * A new `Connection` instance.
    pub fn new(name: String, msg_sender: MsgSender) -> Self {
        Self {
            id: 0,
            name,
            msg_sender,
        }
    }
}

/// Handles a new WebSocket connection.
///
/// This function splits the WebSocket stream into a writer and reader,
/// and then spawns tasks to handle sending and receiving messages.
///
/// # Arguments
///
/// * `ws_stream` - The WebSocket stream associated with the connection.
/// * `mgr_sender` - An unbounded sender used to communicate with the connection manager.
/// * `token_info` - A `String` containing token information related to the connection.
pub async fn handle_connection(
    router: Arc<Router>,
    ws_stream: WebSocketStream<TcpStream>,
    mgr_sender: UnboundedSender<SocketEvents>,
    token_info: String,
) -> Result<()> {
    println!("token info: {}", token_info);
    let (socket_writer, socket_reader) = ws_stream.split();

    // Create a channel for inter-task communication
    let (msg_sender, rx) = mpsc::channel::<Msg>(4);

    let connection = Connection::new(token_info, msg_sender.clone());

    // Spawn a task to handle outgoing messages
    tokio::spawn(recieve_msg(rx, socket_writer));

    // Store the connection ID after the handshake
    let conn_id = connection.id;

    // Send a handshake event to the connection manager
    mgr_sender
        .send(SocketEvents::Handshake(connection))
        .unwrap();

    // Handle incoming messages from the client
    handle_msg(router, socket_reader, msg_sender, mgr_sender, conn_id).await?;
    Ok(())
}

/// Receives messages from the message channel and sends them to the client.
///
/// This function processes messages from the `MsgReciver` and sends them
/// to the WebSocket connection. If an error occurs while sending, the task exits.
///
/// # Arguments
///
/// * `rx` - A receiver channel for incoming messages.
/// * `writer` - The WebSocket writer to send messages to the client.
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

/// Handles incoming messages from the WebSocket reader.
///
/// This function listens for incoming messages, processes them, and sends
/// the processed results back to the client. If the connection is closed,
/// it sends a disconnect event to the connection manager.
///
/// # Arguments
///
/// * `read` - The WebSocket reader stream to receive messages from the client.
/// * `tx` - A sender channel for sending processed messages.
/// * `msg_sender` - An unbounded sender used to communicate with the connection manager.
/// * `connection_id` - The unique ID of the current connection.
async fn handle_msg(
    router: Arc<Router>,
    mut read: SocketReader,
    tx: MsgSender,
    msg_sender: UnboundedSender<SocketEvents>,
    connection_id: u32,
) -> Result<()> {
    while let Some(message) = read.next().await {
        let message = match message {
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
                ));
            } else {
                eprintln!("Header too short: {}", data.len());
            }
        }
    }

    println!("WebSocket connection closed");
    msg_sender
        .send(SocketEvents::Disconnect(connection_id))
        .map_err(|e| Error::CustomError {
            message: format!("Failed to send disconnect event: {}", e),
            line: line!(),
            column: column!(),
        })?;

    Ok(())
}

async fn process_message(cmd: u16, message: BytesMut, router: Arc<Router>, tx: MsgSender) {
    match router.handle_message(cmd, message).await {
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
