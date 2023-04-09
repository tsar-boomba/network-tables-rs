use std::{fmt::Debug, io};

use futures_util::future::BoxFuture;
use tokio_tungstenite::tungstenite::error::ProtocolError;

use super::Topic;

pub struct Config {
    /// milliseconds
    pub connect_timeout: u64,
    /// milliseconds
    pub disconnect_retry_interval: u64,
    pub should_reconnect: Box<dyn Fn(&tokio_tungstenite::tungstenite::Error) -> bool + Send + Sync>,
    pub on_announce: Box<dyn Fn(&Topic) -> BoxFuture<()> + Send + Sync>,
    pub on_un_announce: Box<dyn Fn(Option<Topic>) -> BoxFuture<'static, ()> + Send + Sync>,
    /// Called when there is an error with the websocket and `should_reconnect` returns true
    pub on_disconnect: Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
    pub on_reconnect: Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>,
}

impl Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("connect_timeout", &self.connect_timeout)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect_timeout: 500,
            disconnect_retry_interval: 1000,
            should_reconnect: Box::new(default_should_reconnect),
            on_announce: Box::new(|_| Box::pin(async {})),
            on_un_announce: Box::new(|_| Box::pin(async {})),
            on_disconnect: Box::new(|| Box::pin(async {})),
            on_reconnect: Box::new(|| Box::pin(async {})),
        }
    }
}

pub fn default_should_reconnect(err: &tokio_tungstenite::tungstenite::Error) -> bool {
    match err {
        tokio_tungstenite::tungstenite::Error::AlreadyClosed
        | tokio_tungstenite::tungstenite::Error::ConnectionClosed => true,
        tokio_tungstenite::tungstenite::Error::Protocol(protocol_err) => match protocol_err {
            ProtocolError::SendAfterClosing | ProtocolError::ResetWithoutClosingHandshake => true,
            _ => false,
        },
        tokio_tungstenite::tungstenite::Error::Io(err) => match err.kind() {
            io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::TimedOut => true,
            _ => false,
        },
        _ => true,
    }
}
