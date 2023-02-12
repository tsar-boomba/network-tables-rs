use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::log_result;

use tokio::{
    net::TcpStream,
    sync::Mutex, io::AsyncWriteExt,
};

use super::{client_config::Config, Type, message::Message, EntryData};

#[derive(Debug)]
pub struct Client {
    inner: Arc<InnerClient>,
}

#[derive(Debug)]
struct InnerClient {
    server_addr: SocketAddr,
	entries: Mutex<HashMap<i32, ()>>,
    socket: Mutex<TcpStream>,
    config: Config,
}

impl Client {
    pub async fn try_new_w_config(
        server_addr: impl Into<SocketAddr>,
        config: Config,
    ) -> Result<Self, std::io::Error> {
        // Connect to server
        let server_addr = server_addr.into();

        let socket = TcpStream::connect(&server_addr).await?;

        cfg_tracing! {
            tracing::info!("Connected to {}", server_addr);
        }

        let inner = Arc::new(InnerClient {
            server_addr,
			entries: Mutex::new(HashMap::new()),
            socket: Mutex::new(socket),
            config,
        });
        inner.on_open(&mut *inner.socket.lock().await).await;

        // Task to handle messages from server
        let handle_task_client = Arc::clone(&inner);
        tokio::spawn(async move {
            const TIMESTAMP_INTERVAL: u64 = 5;
            // Start in the past so that first iteration will update the timestamp
            let mut last_time_update = Instant::now()
                .checked_sub(Duration::from_secs(TIMESTAMP_INTERVAL))
                .unwrap();
            loop {
                if Arc::strong_count(&handle_task_client) <= 1 {
                    // If this is the last reference holder, stop
                    break;
                }

                let now = Instant::now();
                if now.duration_since(last_time_update).as_secs() >= TIMESTAMP_INTERVAL {
                    last_time_update = now;
                }

                let mut socket = handle_task_client.socket.lock().await;
                // unwrap should be okay since this "Stream" never ends
                
            }
        });

        Ok(Self { inner })
    }

    pub async fn try_new(
        server_addr: impl Into<SocketAddr>,
    ) -> Result<Self, std::io::Error> {
        Self::try_new_w_config(server_addr, Config::default()).await
    }

    pub async fn new_w_config(server_addr: impl Into<SocketAddr>, config: Config) -> Self {
        Self::try_new_w_config(server_addr, config).await.unwrap()
    }

    pub async fn new(server_addr: impl Into<SocketAddr>) -> Self {
        Self::new_w_config(server_addr, Config::default()).await
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.inner.server_addr
    }

    pub async fn create_entry(
        &self,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    pub async fn delete_entry(&self) -> Result<(), crate::Error> {
        todo!()
    }

    pub async fn set_flags(&self) {
        todo!()
    }

    /// Value should match topic type
    pub async fn update_entry<'a>(
        &self,
        id: u16,
        value: EntryData<'a>,
    ) -> Result<(), crate::Error> {
        todo!()
    }
}

impl InnerClient {
    /// Sends message in websocket, handling reconnection if necessary
    pub(crate) async fn send_message<'a>(&self, message: Message<'a>) -> Result<(), crate::Error> {
        cfg_tracing! {
            tracing::trace!("Sending message: {message:?}");
        }

        let mut socket = self.socket.lock().await;

        loop {
            // somehow not clone message on every iteration???
            match socket.write_all_buf(&mut message.as_bytes()).await {
                Ok(_) => {
                    return Ok(());
                }
                Err(err) => match err.kind() {
                    std::io::ErrorKind::ConnectionAborted => {
                        self.reconnect(&mut socket).await;
                    }
                    std::io::ErrorKind::ConnectionReset => {
                        self.reconnect(&mut socket).await;
                    }
                    _ => return Err(err.into()),
                },
            }
        }
    }

    /// Value should match topic type
    pub(crate) async fn publish_value(
        &self,
        id: i32,
        r#type: Type,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        todo!()
    }

    // Called on connection open, must not fail!
    pub(crate) async fn on_open(&self, socket: &mut TcpStream) {
        
        cfg_tracing! {
            tracing::info!("Prepared new connection.");
        }
    }

    async fn reconnect(&self, socket: &mut TcpStream) {
        (self.config.on_disconnect)();
        loop {
            cfg_tracing! {
                tracing::info!("Attempting reconnect in 500ms");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;

            match TcpStream::connect(self.server_addr).await {
                Ok(new_socket) => {
                    *socket = new_socket;
                    self.on_open(socket).await;
                    (self.config.on_reconnect)();

                    cfg_tracing! {
                        tracing::info!("Successfully reestablished connection.");
                    }
                    break;
                }
                Err(_) => {}
            }
        }
    }
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
