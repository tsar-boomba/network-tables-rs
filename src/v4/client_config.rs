use std::fmt::Debug;

use super::Topic;

pub struct Config {
    /// Milliseconds
    pub connect_timeout: u64,
    pub on_announce: Box<dyn Fn(&Topic) + Send + Sync>,
    pub on_un_announce: Box<dyn Fn(Option<Topic>) + Send + Sync>,
    pub on_disconnect: Box<dyn Fn() + Send + Sync>,
    pub on_reconnect: Box<dyn Fn() + Send + Sync>,
}

impl Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config").finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect_timeout: 100,
            on_announce: Box::new(|_| {}),
            on_un_announce: Box::new(|_| {}),
            on_disconnect: Box::new(|| {}),
            on_reconnect: Box::new(|| {}),
        }
    }
}
