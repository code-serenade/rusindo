use std::sync::Arc;

use bytes::BytesMut;
use rusido::error::Result;
use rusido::websocket;
use service_utils_rs::{services::jwt::Jwt, settings::Settings};

#[tokio::main]
async fn main() -> Result<()> {
    let settings = Settings::new("examples/config/services.toml").unwrap();
    let jwt = Jwt::new(settings.jwt);

    let router = init_router();
    let router = Arc::new(router);

    websocket::server::start(router, jwt).await.unwrap();
    Ok(())
}

fn init_router() -> websocket::router::Router {
    let mut router = websocket::router::Router::new();
    router
        .add_route(1001, handle_user_info)
        .add_route(1002, handle_order);
    router
}

async fn handle_user_info(data: BytesMut) -> Result<BytesMut> {
    // todo others
    let response = BytesMut::from("User Info: John Doe");
    Ok(response)
}

// 定义另一个处理函数
async fn handle_order(data: BytesMut) -> Result<BytesMut> {
    // todo others
    let response = BytesMut::from("Order: #12345");
    Ok(response)
}