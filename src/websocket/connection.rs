use super::router::Router;
use crate::{
    error::{Error, Result},
    websocket::{events::SocketEvents, header_parser::HeaderParser},
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
        mpsc::{self, Receiver, Sender, UnboundedSender},
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
type MsgSender = Sender<Msg>;
/// Receiver type alias for receiving `Msg` in a task.
type MsgReciver = Receiver<Msg>;

/// Represents a client connection.
///
/// # Fields
/// - `id`: The unique identifier for the connection.
/// - `msg_sender`: The sender used to send messages to the connection.
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: u32,
    pub msg_sender: MsgSender,
}

impl Connection {
    /// Creates a new `Connection`.
    ///
    /// # Arguments
    /// - `id`: The unique identifier for the connection.
    /// - `msg_sender`: The sender used to send messages to the connection.
    ///
    /// # Returns
    /// A new `Connection` instance.
    pub fn new(id: u32, msg_sender: MsgSender) -> Self {
        Self { id, msg_sender }
    }
}

/// Handles the initialization and setup of a WebSocket connection.
///
/// This function sets up the message channels, performs the handshake process,
/// and starts message handling if the handshake is successful.
///
/// # Arguments
/// - `router`: The router to handle message routing.
/// - `ws_stream`: The WebSocket stream representing the connection.
/// - `mgr_sender`: The sender for sending events to the connection manager.
/// - `id`: The unique ID of the connection.
///
/// # Returns
/// `Result<()>` indicating success or failure.
pub async fn handle_connection(
    router: Arc<Router>,
    ws_stream: WebSocketStream<TcpStream>,
    mgr_sender: UnboundedSender<SocketEvents>,
    id: u32,
) -> Result<()> {
    println!("socket id: {}", id);

    // Message channel
    let (msg_sender, msg_reciever) = mpsc::channel::<Msg>(4);

    let (tx, rx) = oneshot::channel::<u16>();

    let connection = Connection::new(id, msg_sender.clone());

    // Send a handshake event to the connection manager
    mgr_sender
        .send(SocketEvents::Handshake(tx, connection.clone()))
        .unwrap();

    process_handshake(rx, ws_stream, router, connection, msg_reciever).await
}

/// Handles the handshake process and determines whether to proceed with message handling.
///
/// # Arguments
/// - `rx`: The receiver that waits for the handshake result.
/// - `ws_stream`: The WebSocket stream representing the connection.
/// - `router`: The router to handle message routing.
/// - `connection`: The client connection instance.
/// - `msg_reciever`: The receiver for incoming messages.
///
/// # Returns
/// `Result<()>` indicating success or failure.
async fn process_handshake(
    rx: oneshot::Receiver<u16>,
    ws_stream: WebSocketStream<TcpStream>,
    router: Arc<Router>,
    connection: Connection,
    msg_reciever: MsgReciver,
) -> Result<()> {
    match rx.await {
        Ok(error_code) => match error_code {
            0 => {
                let (socket_writer, socket_reader) = ws_stream.split();
                tokio::spawn(recieve_msg(msg_reciever, socket_writer));
                handle_msg(router, socket_reader, connection).await
            }
            _ => Ok(()),
        },
        Err(_) => Ok(()),
    }
}

/// Handles receiving messages from the `msg_reciever` and sending them through the WebSocket connection.
///
/// # Arguments
/// - `rx`: The receiver for incoming messages.
/// - `writer`: The writer half of the WebSocket connection.
///
/// # Returns
/// `Result<()>` indicating success or failure.
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

/// Handles incoming messages from the WebSocket connection.
///
/// This function processes incoming binary messages, parses them, and spawns a task to handle the message content.
///
/// # Arguments
/// - `router`: The router to handle message routing.
/// - `read`: The reader half of the WebSocket connection.
/// - `connection`: The client connection instance.
///
/// # Returns
/// `Result<()>` indicating success or failure.
async fn handle_msg(
    router: Arc<Router>,
    mut read: SocketReader,
    connection: Connection,
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
                    connection.msg_sender.clone(),
                ));
            } else {
                eprintln!("Header too short: {}", data.len());
            }
        }
    }

    println!("WebSocket connection closed");
    connection
        .msg_sender
        .send((0, 0, None))
        .await
        .map_err(|e| Error::CustomError {
            message: format!("Failed to send disconnect event: {}", e),
            line: line!(),
            column: column!(),
        })?;

    Ok(())
}

/// Processes a single message and sends a response if necessary.
///
/// # Arguments
/// - `cmd`: The command identifier.
/// - `message`: The message payload.
/// - `router`: The router to handle message routing.
/// - `tx`: The sender to send responses back.
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

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[tokio::test]
    async fn test_router_add_and_handle_route() {
        let mut router = Router::new();

        // Define a handler function for a specific command ID (e.g., 42)
        router.add_route(42, |data: BytesMut| async move {
            let mut response = BytesMut::from("Echo: ");
            response.extend_from_slice(&data);
            Ok(response)
        });

        // Create test data
        let input_data = BytesMut::from("Hello, World!");

        // Handle the message with command ID 42
        let result = router.handle_message(42, input_data.clone()).await;

        // Assert that the result is correct
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), BytesMut::from("Echo: Hello, World!"));

        // Test with an unknown command ID (e.g., 99)
        let result = router.handle_message(99, input_data).await;

        // Assert that the result is an error
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_router_no_route() {
        let router = Router::new();

        // Test with no routes added
        let result = router.handle_message(1, BytesMut::new()).await;

        // Assert that the result is an error
        assert!(result.is_err());
    }
}
