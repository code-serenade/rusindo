use crate::error::{Error, Result};
use futures::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::{net::TcpStream, time};
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message, MaybeTlsStream, WebSocketStream,
};

type ClientSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// WebSocket 客户端结构体
pub struct WebSocketClient {
    url: String,
    socket: Option<ClientSocket>, // 使用 Option 来表示 WebSocket 连接是否存在
}

impl WebSocketClient {
    /// 创建一个新的 WebSocket 客户端
    pub async fn new(url: String) -> Result<Self> {
        let client = WebSocketClient {
            url,
            socket: None, // 初始时没有连接
        };
        Ok(client)
    }

    /// 连接到 WebSocket 服务器
    async fn connect(&mut self) -> Result<()> {
        let (socket, _) = connect_async(&self.url).await?;
        self.socket = Some(socket);
        println!("成功连接到 WebSocket 服务器");
        Ok(())
    }

    /// 发送消息到 WebSocket 服务器
    pub async fn send_message(&mut self, msg: String) -> Result<()> {
        if let Some(socket) = &mut self.socket {
            let msg = Message::Text(msg.into());
            socket.send(msg).await?;
            println!("已发送消息");
            Ok(())
        } else {
            Err(Error::ErrorMessage("WebSocket 未连接".to_string()))
        }
    }

    /// 接收 WebSocket 服务器的消息
    pub async fn receive_message(&mut self) -> Result<()> {
        loop {
            // 如果 WebSocket 连接丢失，则重连
            if self.socket.is_none() {
                println!("WebSocket 连接丢失，正在重连...");
                // 这里移除了 sleep 和重连逻辑，直接调用 reconnect
                self.reconnect().await?;
            }

            // 接收消息
            if let Some(socket) = &mut self.socket {
                match socket.next().await {
                    Some(Ok(Message::Text(text))) => {
                        println!("收到消息: {}", text);
                    }
                    Some(Ok(Message::Binary(_))) => {
                        println!("收到二进制消息");
                    }
                    Some(Ok(Message::Ping(_))) => {
                        println!("收到 Ping 消息");
                    }
                    Some(Ok(Message::Pong(_))) => {
                        println!("收到 Pong 消息");
                    }
                    Some(Ok(Message::Close(_))) => {
                        println!("连接关闭");
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {
                        println!("收到帧消息");
                    }
                    Some(Err(e)) => {
                        println!("接收消息时出错: {}", e);
                        break;
                    }
                    None => {
                        println!("没有更多的消息");
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// 尝试重连 WebSocket
    async fn reconnect(&mut self) -> Result<()> {
        // 重连逻辑
        let mut retries = 5; // 最大重连次数
        while retries > 0 {
            match self.connect().await {
                Ok(_) => {
                    println!("重连成功");
                    return Ok(()); // 成功重连后直接返回
                }
                Err(e) => {
                    retries -= 1;
                    println!("重连失败，剩余重试次数: {}, 错误: {}", retries, e);
                    if retries > 0 {
                        time::sleep(Duration::from_secs(5)).await; // 失败时等待 5 秒后重试
                    }
                }
            }
        }
        Err(Error::ErrorMessage(
            "重连失败，已达最大重试次数".to_string(),
        ))
    }
}
