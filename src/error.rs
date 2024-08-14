use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("service utils error: {0}")]
    JwtError(#[from] service_utils_rs::error::Error),

    #[error("websocket error: {0}")]
    WsError(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("{message:} ({line:}, {column})")]
    CustomError {
        message: String,
        line: u32,
        column: u32,
    },

    #[error("error code: {0}")]
    ErrorCode(u16),
}

pub type Result<T, E = Error> = core::result::Result<T, E>;
