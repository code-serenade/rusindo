use rusido::error::Result;
use rusido::websocket::client::WebSocketClient; // 替换为你的 crate 路径 // 替换为你的错误类型

#[tokio::main]
async fn main() -> Result<()> {
    // 创建 WebSocket 客户端实例
    let mut client = WebSocketClient::new("ws://localhost:13785".to_string()).await?;

    // 连接到 WebSocket 服务器
    client.connect().await?;

    // 发送一条消息
    client
        .send_message("Hello, WebSocket server!".to_string())
        .await?;

    // 接收来自服务器的消息
    // client.receive_message().await?;

    Ok(())
}
