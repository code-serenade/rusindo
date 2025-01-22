pub enum SendMessage {
    Ping(),
    Binary(Vec<u8>),
    Text(String),
    Json(String),
}
