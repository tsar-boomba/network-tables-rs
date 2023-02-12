use std::fmt::Debug;

#[derive()]
pub struct Config {
    pub on_announce: Box<dyn Fn(()) + Send + Sync>,
    pub on_un_announce: Box<dyn Fn(Option<()>) + Send + Sync>,
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
            on_announce: Box::new(|_| {}),
            on_un_announce: Box::new(|_| {}),
            on_disconnect: Box::new(|| {}),
            on_reconnect: Box::new(|| {}),
        }
    }
}
