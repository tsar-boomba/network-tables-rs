use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[cfg(feature = "__v4")]
    #[error("WebSocket error: {0:?}")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[cfg(feature = "__v4")]
    #[error("Json error: {0:?}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("Io error: {0:?}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "__v3")]
    #[error("Leb read error: {0:?}")]
    LebRead(#[from] leb128::read::Error),
    #[cfg(feature = "__v3")]
    #[error("From utf8 error: {0:?}")]
    FromUtf8(#[from] std::string::FromUtf8Error),

    #[error("Timed out connecting to server")]
    ConnectTimeout(#[from] tokio::time::error::Elapsed),
    // Server error
    #[error("Server responded with an invalid type of message")]
    InvalidMessageType(&'static str),
}
