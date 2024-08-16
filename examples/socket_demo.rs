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

    websocket::server::start(10031, router, jwt, sub_to_id)
        .await
        .unwrap();
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
    println!("data: {:?}", data);
    let response = BytesMut::from("User Info: John Doe");
    Ok(response)
}

// 定义另一个处理函数
async fn handle_order(data: BytesMut) -> Result<BytesMut> {
    // todo others
    println!("data: {:?}", data);
    let response = BytesMut::from("Order: #12345");
    Ok(response)
}

fn sub_to_id(sub: &str) -> u32 {
    match sub.parse::<u32>() {
        Ok(id) => id,
        Err(_) => 300,
    }
}
