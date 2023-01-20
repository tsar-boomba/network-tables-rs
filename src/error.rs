use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("WebSocket error: {0:?}")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("Json error: {0:?}")]
    SerdeJson(#[from] serde_json::Error),

    // Server error
    #[error("Server responded with an invalid type of message")]
    InvalidMessageType(&'static str),
}
