use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt::Display,
    net::SocketAddr,
    ops::Div,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use super::{
    Announce, InternalSub, MessageData, NTMessage, PublishProperties, PublishTopic, PublishedTopic,
    SetProperties, Subscribe, Subscription, SubscriptionData, SubscriptionOptions, Type, Topic,
};
use futures_util::{poll, SinkExt, StreamExt};
use tokio::{
    net::TcpStream,
    sync::{mpsc, Mutex},
};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};

#[derive(Debug)]
pub struct Client {
    inner: Arc<InnerClient>,
}

type WebSocket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

#[derive(Debug)]
struct InnerClient {
    server_addr: SocketAddr,
    // Keys are subuid, value is a handle to sub data and a sender to the sub's mpsc
    subscriptions: Mutex<HashMap<i32, InternalSub>>,
    announced_topics: Mutex<HashMap<i32, Topic>>,
    client_published_topics: Mutex<HashMap<i32, PublishedTopic>>,
    socket: tokio::sync::Mutex<WebSocket>,
    server_time_offset: parking_lot::Mutex<u32>,
    start_time: Instant,
    sub_counter: parking_lot::Mutex<i32>,
    topic_counter: parking_lot::Mutex<i32>,
}

impl Client {
    pub async fn new(server_addr: impl Into<SocketAddr>) -> Self {
        Self::try_new(server_addr).await.unwrap()
    }

    pub async fn try_new(
        server_addr: impl Into<SocketAddr>,
    ) -> Result<Self, tokio_tungstenite::tungstenite::Error> {
        // Connect to server
        let server_addr = server_addr.into();
        let mut request = format!(
            "ws://{server_addr}/nt/rust-client-{}",
            rand::random::<u32>()
        )
        .into_client_request()?;
        // Add sub-protocol header
        request.headers_mut().append(
            "Sec-WebSocket-Protocol",
            HeaderValue::from_static("networktables.first.wpi.edu"),
        );
        let uri = request.uri().clone();

        let (socket, _) = tokio_tungstenite::connect_async(request).await?;

        cfg_tracing! {
            tracing::info!("Connected to {}", uri);
        }

        let inner = Arc::new(InnerClient {
            server_addr,
            subscriptions: Mutex::new(HashMap::new()),
            announced_topics: Mutex::new(HashMap::new()),
            client_published_topics: Mutex::new(HashMap::new()),
            socket: Mutex::new(socket),
            server_time_offset: parking_lot::Mutex::new(0),
            start_time: Instant::now(),
            sub_counter: parking_lot::Mutex::new(1),
            topic_counter: parking_lot::Mutex::new(1),
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
                    handle_task_client.update_time().await;
                }

                let mut socket = handle_task_client.socket.lock().await;
                // unwrap should be okay since this "Stream" never ends
                match poll!(socket.next()) {
                    std::task::Poll::Ready(Some(Ok(message))) => {
                        cfg_tracing! {
                            tracing::trace!("Message received from server.");
                        }
                        // Spawn task to handle the message
                        tokio::spawn(handle_message(Arc::clone(&handle_task_client), message));
                    }
                    std::task::Poll::Ready(Some(Err(err))) => match err {
                        tokio_tungstenite::tungstenite::Error::AlreadyClosed => {
                            handle_task_client.reconnect(&mut socket).await;
                        }
                        tokio_tungstenite::tungstenite::Error::ConnectionClosed => {
                            handle_task_client.reconnect(&mut socket).await;
                        }
                        _ => {}
                    },
                    _ => {}
                };
            }
        });

        Ok(Self { inner })
    }

    pub fn server_addr(&self) -> SocketAddr {
        self.inner.server_addr
    }

    pub async fn publish_topic(
        &self,
        name: impl AsRef<str>,
        topic_type: Type,
        properties: Option<PublishProperties>,
    ) -> Result<PublishedTopic, crate::Error> {
        let pubuid = self.inner.new_topic_id();
        let mut messages: Vec<NTMessage> = Vec::with_capacity(2);
        let publish_message = NTMessage::Publish(PublishTopic {
            name: name.as_ref(),
            pubuid,
            r#type: topic_type.clone(),
            properties: Cow::Borrowed(&properties),
        });

        if let Some(properties) = &properties {
            messages[0] = publish_message;
            messages[1] = NTMessage::SetProperties(SetProperties {
                name: name.as_ref(),
                update: Cow::Borrowed(properties),
            });
        } else {
            messages[0] = publish_message;
        };
        messages.shrink_to_fit();

        // Put message in an array and serialize
        let message = serde_json::to_string(&messages)?;

        log_result(self.inner.send_message(Message::Text(message)).await)?;

        let topic = PublishedTopic {
            name: name.as_ref().to_owned(),
            pubuid,
            r#type: topic_type,
            properties,
        };

        self.inner
            .client_published_topics
            .lock()
            .await
            .insert(pubuid, topic.clone());

        Ok(topic)
    }

    pub async fn unpublish(&self, topic: PublishedTopic) -> Result<(), crate::Error> {
        // Put message in an array and serialize
        let message = serde_json::to_string(&[topic.as_unpublish()])?;

        log_result(self.inner.send_message(Message::Text(message)).await)?;

        Ok(())
    }

    pub async fn set_properties(&self) {
        todo!()
    }

    pub async fn subscribe(
        &self,
        topic_names: &[impl ToString],
    ) -> Result<Subscription, crate::Error> {
        self.subscribe_w_options(topic_names, None).await
    }

    pub async fn subscribe_w_options(
        &self,
        topic_names: &[impl ToString],
        options: Option<SubscriptionOptions>,
    ) -> Result<Subscription, crate::Error> {
        let topic_names: Vec<String> = topic_names.into_iter().map(ToString::to_string).collect();
        let subuid = self.inner.new_sub_id();

        // Put message in an array and serialize
        let message = serde_json::to_string(&[NTMessage::Subscribe(Subscribe {
            subuid,
            topics: HashSet::from_iter(topic_names.iter().cloned()),
            options: options.clone(),
        })])?;

        log_result(self.inner.send_message(Message::Text(message)).await)?;

        let data = Arc::new(SubscriptionData {
            options: options,
            subuid,
            topics: HashSet::from_iter(topic_names.into_iter()),
        });

        let (sender, receiver) = mpsc::channel::<MessageData>(100);
        self.inner.subscriptions.lock().await.insert(
            subuid,
            InternalSub {
                data: Arc::downgrade(&data),
                sender,
            },
        );

        Ok(Subscription { data, receiver })
    }

    pub async fn unsubscribe(&self, sub: Subscription) -> Result<(), crate::Error> {
        // Put message in an array and serialize
        let message = serde_json::to_string(&[sub.as_unsubscribe()])?;
        log_result(self.inner.send_message(Message::Text(message)).await)?;

        // Remove from our subscriptions
        self.inner
            .subscriptions
            .lock()
            .await
            .remove(&sub.data.subuid);

        Ok(())
    }

    pub async fn publish_value_w_timestamp(
        &self,
        topic: &Topic,
        timestamp: u32,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        self.inner
            .publish_value_w_timestamp(topic, timestamp, value)
            .await
    }

    /// Value should match topic type
    pub async fn publish_value(
        &self,
        topic: &Topic,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        self.inner.publish_value(topic, value).await
    }
}

impl InnerClient {
    /// Sends message in websocket, handling reconnection if necessary
    pub(crate) async fn send_message(&self, message: Message) -> Result<(), crate::Error> {
        cfg_tracing! {
            tracing::trace!("Sending message: {message:?}");
        }

        let mut socket = self.socket.lock().await;

        loop {
            // somehow not clone message on every iteration???
            match socket.send(message.clone()).await {
                Ok(_) => {
                    return Ok(());
                }
                Err(err) => match err {
                    tokio_tungstenite::tungstenite::Error::AlreadyClosed => {
                        self.reconnect(&mut socket).await;
                    }
                    tokio_tungstenite::tungstenite::Error::ConnectionClosed => {
                        self.reconnect(&mut socket).await;
                    }
                    _ => return Err(err.into()),
                },
            }
        }
    }

    #[inline]
    pub(crate) fn client_time(&self) -> u32 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u32
    }

    pub(crate) fn server_time(&self) -> u32 {
        self.client_time() + *self.server_time_offset.lock()
    }

    /// Takes new timestamp value and updates this client's offset
    pub(crate) fn handle_new_timestamp(
        &self,
        server_timestamp: u32,
        client_timestamp: Option<i32>,
    ) {
        if let Some(client_timestamp) = client_timestamp {
            let receive_time = self.client_time();
            tracing::trace!("Received at: {receive_time}\nCient time: {client_timestamp}");
            let round_trip_time = receive_time - client_timestamp as u32;
            tracing::trace!("ett: {round_trip_time}");
            let server_time_at_receive = server_timestamp - round_trip_time.div(2) as u32;
            tracing::trace!("Server time at receive: {server_time_at_receive}");
            *self.server_time_offset.lock() = server_time_at_receive - receive_time;
        }
    }

    pub(crate) fn new_topic_id(&self) -> i32 {
        let mut current_id = self.topic_counter.lock();
        let new_id = current_id.checked_add(1).unwrap_or(1);
        *current_id = new_id;
        new_id
    }

    pub(crate) fn new_sub_id(&self) -> i32 {
        let mut current_id = self.sub_counter.lock();
        let new_id = current_id.checked_add(1).unwrap_or(1);
        *current_id = new_id;
        new_id
    }

    pub(crate) async fn publish_value_w_timestamp(
        &self,
        topic: &Topic,
        timestamp: u32,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        let mut buf = Vec::<u8>::with_capacity(19);

        // TODO: too lazy to handle these errors 😴
        rmp::encode::write_array_len(&mut buf, 4).unwrap();
        // Client side topic is guaranteed to have a uid
        rmp::encode::write_i32(&mut buf, topic.pubuid.unwrap()).unwrap();
        rmp::encode::write_u32(&mut buf, timestamp).unwrap();
        rmp::encode::write_i32(&mut buf, topic.r#type.as_u8() as i32).unwrap();
        rmpv::encode::write_value(&mut buf, value).unwrap();

        self.send_message(Message::Binary(buf)).await
    }

    /// Value should match topic type
    pub(crate) async fn publish_value(
        &self,
        topic: &Topic,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        self.publish_value_w_timestamp(topic, self.server_time(), value)
            .await
    }

    pub(crate) async fn update_time(&self) {
        let announced_topics = self.announced_topics.lock().await;
        let time_topic = announced_topics.get(&-1);

        if let Some(time_topic) = time_topic {
            cfg_tracing! {
                tracing::trace!("Updating timestamp.");
            }

            log_result(
                self.publish_value_w_timestamp(
                    time_topic,
                    0,
                    &rmpv::Value::Integer(self.client_time().into()),
                )
                .await,
            )
            .ok();
        }
    }

    // Called on connection open, must not fail!
    pub(crate) async fn on_open(&self, socket: &mut WebSocket) {
        let mut announced = self.announced_topics.lock().await;
        let client_published = self.client_published_topics.lock().await;
        let mut subscriptions = self.subscriptions.lock().await;
        announced.insert(
            -1,
            Topic {
                id: -1,
                name: "Time".into(),
                pubuid: Some(-1),
                r#type: Type::Int,
                properties: None,
            },
        );

        // One allocation
        let mut messages: Vec<NTMessage> =
            Vec::with_capacity(client_published.len() + subscriptions.len());

        // Add publish messages
        client_published
            .values()
            .enumerate()
            .for_each(|(i, topic)| {
                messages[i] = NTMessage::Publish(PublishTopic {
                    name: &topic.name,
                    properties: Cow::Borrowed(&topic.properties),
                    // Client published is guaranteed to have a uid
                    pubuid: topic.pubuid,
                    r#type: topic.r#type,
                });
            });

        // Remove invalid subs (user has dropped them)
        subscriptions.retain(|_, sub| !sub.is_valid());

        // Add subscribe messages
        messages.extend(subscriptions.values().filter_map(|sub| {
            if let Some(data) = sub.data.upgrade() {
                return Some(NTMessage::Subscribe(Subscribe {
                    subuid: data.subuid,
                    // Somehow get rid of cloning here?
                    topics: data.topics.clone(),
                    options: data.options.clone(),
                }));
            }
            None
        }));

        // Send all messages at once (please don't fail 🥺)
        socket
            .send(Message::Text(serde_json::to_string(&messages).unwrap()))
            .await
            .ok();

        cfg_tracing! {
            tracing::info!("Prepared new connection.");
        }
    }

    async fn reconnect(&self, socket: &mut WebSocket) {
        loop {
            cfg_tracing! {
                tracing::info!("Attempting reconnect in 500ms");
            }
            tokio::time::sleep(Duration::from_millis(500)).await;

            let mut request = format!("ws://{}/nt/rust-client", self.server_addr)
                .into_client_request()
                .unwrap();
            // Add sub-protocol header
            request.headers_mut().append(
                "Sec-WebSocket-Protocol",
                HeaderValue::from_static("networktables.first.wpi.edu"),
            );

            match tokio_tungstenite::connect_async(request).await {
                Ok((new_socket, _)) => {
                    *socket = new_socket;
                    self.on_open(socket).await;

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

#[inline(always)]
fn log_result<T, E: Display>(result: Result<T, E>) -> Result<T, E> {
    #[cfg(feature = "tracing")]
    match &result {
        Err(err) => ::tracing::error!("{}", err),
        _ => {}
    };
    result
}

/// Handles messages from the server
async fn handle_message(client: Arc<InnerClient>, message: Message) {
    tracing::trace!("Handling message");
    match message {
        Message::Text(message) => {
            // Either announce, unannounce, or properties
            let messages: Vec<NTMessage> = match log_result(
                serde_json::from_str(&message).map_err(Into::<crate::Error>::into),
            ) {
                Ok(messages) => messages,
                Err(_) => {
                    cfg_tracing! {tracing::error!("Server sent an invalid message: {message:?}");}
                    return;
                }
            };

            for message in messages {
                match message {
                    NTMessage::Announce(Announce {
                        name,
                        id,
                        pubuid,
                        properties,
                        r#type,
                    }) => {
                        let mut announced = client.announced_topics.lock().await;

                        cfg_tracing! {
                            tracing::info!("Server announced: {name}");
                        }

                        if let Some(existing) = announced.get_mut(&id) {
                            // use server's pubuid if it sent one
                            if pubuid.is_some() {
                                existing.pubuid = pubuid;
                            };
                        } else {
                            announced.insert(
                                id,
                                Topic {
                                    name: name.to_owned(),
                                    id,
                                    pubuid,
                                    properties: Some(properties),
                                    r#type,
                                },
                            );
                        }
                    }
                    NTMessage::UnAnnounce(un_announce) => {
                        cfg_tracing! {
                            tracing::info!("Server un_announced: {}", un_announce.name);
                        }

                        client.announced_topics.lock().await.remove(&un_announce.id);
                    }
                    NTMessage::Properties(_) => {
                        // I don't need to do anything
                    }
                    _ => {
                        cfg_tracing! {tracing::error!("Server sent an invalid message: {message:?}");}
                    }
                }
            }
        }
        Message::Binary(msgpack) => {
            // Message pack value, update
            let data = match rmp_serde::decode::from_slice(&msgpack) {
                Ok(data) => data,
                Err(_) => {
                    cfg_tracing! {
                        tracing::error!("Server sent an invalid msgpack data: {msgpack:?}");
                    }
                    return;
                }
            };

            match data {
                rmpv::Value::Array(array) => {
                    if array.len() != 4 {
                        cfg_tracing! {
                            tracing::error!("Server sent an invalid msgpack data, wrong length.");
                        }
                        return;
                    }

                    let id = array[0].as_i64().map(|n| n as i32);
                    let timestamp_micros = array[1].as_u64().map(|n| n as u32);
                    let type_idx = array[2].as_u64();
                    let data = &array[3];

                    if let Some(id) = id {
                        if let Some(timestamp_micros) = timestamp_micros {
                            if id >= 0 {
                                if let Some(type_idx) = type_idx {
                                    let r#type = Type::from_num(type_idx);
                                    if let Some(r#type) = r#type {
                                        if let Some(topic) =
                                            client.announced_topics.lock().await.get(&(id as i32))
                                        {
                                            client.subscriptions.lock().await.retain(|_, sub| {
                                                if !sub.is_valid() {
                                                    false
                                                } else {
                                                    if sub.matches_topic(topic) {
                                                        sub.sender
                                                            .try_send(MessageData {
                                                                topic_name: topic.name.clone(),
                                                                timestamp: timestamp_micros,
                                                                r#type: r#type.clone(),
                                                                data: data.to_owned(),
                                                            })
                                                            .is_ok()
                                                    } else {
                                                        false
                                                    }
                                                }
                                            });
                                        } else {
                                            // Topic wasn't previously announced, ignoring it
                                        }
                                    } else {
                                        // Invalid type id
                                        cfg_tracing! {
                                            tracing::error!("Server sent an invalid type id");
                                        }
                                    }
                                }
                            } else if id == -1 {
                                // Timestamp update
                                client.handle_new_timestamp(
                                    timestamp_micros,
                                    data.as_i64().map(|n| n as i32),
                                );
                            } else {
                                // Invalid id
                                cfg_tracing! {
                                    tracing::error!("Server sent an invalid topic id, less than -1");
                                }
                            };

                            return;
                        }

                        return;
                    }
                }
                _ => {
                    cfg_tracing! {
                        tracing::error!("Server sent an invalid msgpack data, not an array.");
                    }
                }
            }
        }
        _ => {}
    }
}
