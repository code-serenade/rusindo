use super::{
    connection::Connection,
    error_code::{SUCCECE, SYSTEM_ERROR},
    events::SocketEvents,
};
use crate::error::{Error, Result};
use bytes::BytesMut;
use std::collections::HashMap;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct SocketManager {
    connections: HashMap<u32, Connection>,
    max_client: usize,
}

impl SocketManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            max_client: 10000,
        }
    }

    pub fn add_connection(&mut self, conn: Connection) -> Result<()> {
        if self.connections.len() >= self.max_client {
            return Err(Error::SystemError("Too many connections".to_string()));
        }
        self.connections.insert(conn.id, conn);

        Ok(())
    }

    pub fn remove_connection(&mut self, id: u32) {
        println!("===============remove connection=========: {}", id);
        self.connections.remove(&id).unwrap();
    }

    /// 广播消息给指定或所有连接的客户端
    pub async fn broadcast_message(
        &self,
        error_code: u16,
        cmd: u16,
        message: Option<BytesMut>,
        connection_ids: Option<Vec<u32>>,
    ) {
        match connection_ids {
            Some(ids) => {
                // 发送给指定的连接
                for id in ids {
                    if let Some(connection) = self.connections.get(&id) {
                        if let Err(e) = connection
                            .msg_sender
                            .send((error_code, cmd, message.clone()))
                            .await
                        {
                            eprintln!("Failed to send message to connection {}: {}", id, e);
                        }
                    }
                }
            }
            None => {
                // 发送给所有连接
                for (id, connection) in &self.connections {
                    if let Err(e) = connection
                        .msg_sender
                        .send((error_code, cmd, message.clone()))
                        .await
                    {
                        eprintln!("Failed to send message to connection {}: {}", id, e);
                    }
                }
                println!(
                    "Broadcast message sent to {} clients",
                    self.connections.len()
                );
            }
        }
    }
}

pub async fn start_loop(mut reciever: UnboundedReceiver<SocketEvents>) -> Result<()> {
    let mut mgr = SocketManager::new();
    while let Some(event) = reciever.recv().await {
        match event {
            SocketEvents::Handshake(tx, conn) => match mgr.add_connection(conn) {
                Err(_e) => {
                    tx.send(SYSTEM_ERROR).unwrap();
                }
                Ok(_) => tx.send(SUCCECE).unwrap(),
            },
            SocketEvents::Disconnect(id) => mgr.remove_connection(id),
        }
    }
    Ok(())
}
