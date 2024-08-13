use super::{connection, events::SocketEvents, manager, router::Router};
use crate::error::Result;
use service_utils_rs::{services::jwt::Jwt, utils::string_util::QueryExtractor};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        http,
    },
};

pub async fn start(router: Arc<Router>, jwt: Jwt) -> Result<()> {
    let addr = "0.0.0.0:10301".to_string();
    let listener = TcpListener::bind(&addr).await?;

    println!("WebSocket Server is running on ws://{}", addr);

    let (sender, receiver) = mpsc::unbounded_channel::<SocketEvents>();

    tokio::spawn(manager::start_loop(receiver));

    while let Ok((stream, client_addr)) = listener.accept().await {
        let mut token_info = String::new();

        let callback = |req: &Request, mut res: Response| {
            if let Some(token) = req
                .uri()
                .query()
                .and_then(|query| query.extract_value("token").map(|t| t.to_string()))
            {
                match jwt.validate_access_token(&token) {
                    Ok(claims) => {
                        println!("claims: {:?}", claims);
                        token_info.push_str(&claims.sub);
                    }
                    Err(_) => *res.status_mut() = http::StatusCode::BAD_REQUEST,
                }
            } else {
                *res.status_mut() = http::StatusCode::BAD_REQUEST;
            }
            Ok(res)
        };

        match accept_hdr_async(stream, callback).await {
            Err(e) => println!("Websocket connection error : {}", e),
            Ok(ws_stream) => {
                println!("New client addr: {}", client_addr);
                tokio::spawn(connection::handle_connection(
                    router.clone(),
                    ws_stream,
                    sender.clone(),
                    token_info,
                ));
            }
        }
    }

    Ok(())
}
