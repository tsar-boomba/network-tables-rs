use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    ops::Div,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use crate::log_result;

use super::{
    Announce, Config, InternalSub, MessageData, NTMessage, PublishProperties, PublishTopic,
    PublishedTopic, SetProperties, Subscribe, Subscription, SubscriptionData, SubscriptionOptions,
    Topic, Type,
};
use futures_util::{SinkExt, TryStreamExt};
use tokio::{
    net::TcpStream,
    select,
    sync::{mpsc, Mutex},
    task::yield_now,
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
    client_published_topics: Mutex<HashMap<u32, PublishedTopic>>,
    socket_sender: mpsc::Sender<Message>,
    server_time_offset: parking_lot::Mutex<u32>,
    sub_counter: parking_lot::Mutex<i32>,
    topic_counter: parking_lot::Mutex<u32>,
    config: Config,
    // Has to be mutable to prevent overflow if it becomes too long ago
    start_time: parking_lot::Mutex<Instant>,
}

impl Client {
    pub async fn try_new_w_config(
        server_addr: impl Into<SocketAddr>,
        config: Config,
    ) -> Result<Self, crate::Error> {
        let (socket_sender, socket_receiver) = mpsc::channel::<Message>(100);
        let inner = Arc::new(InnerClient {
            server_addr: server_addr.into(),
            subscriptions: Mutex::new(HashMap::new()),
            announced_topics: Mutex::new(HashMap::new()),
            client_published_topics: Mutex::new(HashMap::new()),
            socket_sender,
            server_time_offset: parking_lot::Mutex::new(0),
            sub_counter: parking_lot::Mutex::new(0),
            topic_counter: parking_lot::Mutex::new(0),
            start_time: parking_lot::Mutex::new(Instant::now()),
            config,
        });
        setup_socket(Arc::downgrade(&inner), socket_receiver).await?;

        inner.on_open().await;

        // Task to handle messages from server
        let timestamp_task_client = Arc::downgrade(&inner);
        tokio::spawn(async move {
            const TIMESTAMP_INTERVAL: u64 = 5;
            loop {
                match timestamp_task_client.upgrade() {
                    Some(client) => client.update_time().await.ok(),
                    None => break,
                };

                tokio::time::sleep(Duration::from_secs(TIMESTAMP_INTERVAL)).await;
            }
        });

        Ok(Self { inner })
    }

    pub async fn try_new(server_addr: impl Into<SocketAddr>) -> Result<Self, crate::Error> {
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
            messages.push(publish_message);
            messages.push(NTMessage::SetProperties(SetProperties {
                name: name.as_ref(),
                update: Cow::Borrowed(properties),
            }));
        } else {
            messages.push(publish_message);
        };

        // Put message in an array and serialize
        let message = serde_json::to_string(&messages)?;

        self.inner.send_message(Message::Text(message)).await;

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

        self.inner.send_message(Message::Text(message)).await;

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

        self.inner.send_message(Message::Text(message)).await;

        let data = Arc::new(SubscriptionData {
            options: options,
            subuid,
            topics: HashSet::from_iter(topic_names.into_iter()),
        });

        let (sender, receiver) = mpsc::channel::<MessageData>(256);
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
        self.inner.send_message(Message::Text(message)).await;

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
        topic: &PublishedTopic,
        timestamp: u32,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        self.inner
            .publish_value_w_timestamp(
                UnsignedIntOrNegativeOne::UnsignedInt(topic.pubuid),
                topic.r#type,
                timestamp,
                value,
            )
            .await
    }

    /// Value should match topic type
    pub async fn publish_value(
        &self,
        topic: &PublishedTopic,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        self.inner
            .publish_value(
                UnsignedIntOrNegativeOne::UnsignedInt(topic.pubuid),
                topic.r#type,
                value,
            )
            .await
    }

    pub async fn use_announced_topics<F: Fn(&HashMap<i32, Topic>)>(&self, f: F) {
        f(&*self.inner.announced_topics.lock().await)
    }
}

impl InnerClient {
    /// Sends message in websocket, handling reconnection if necessary
    pub(crate) async fn send_message(&self, message: Message) {
        cfg_tracing! {
            tracing::trace!("Sending message: {message:?}");
        }

        // Should never be dropped before a send goes off
        self.socket_sender.send(message).await.unwrap();
    }

    #[inline]
    pub(crate) fn client_time(&self) -> u32 {
        Instant::now()
            .duration_since(*self.start_time.lock())
            .as_micros() as u32
    }

    pub(crate) fn server_time(&self) -> u32 {
        self.client_time() + *self.server_time_offset.lock()
    }

    /// Takes new timestamp value and updates this client's offset
    /// Returns `None` if the math failed
    pub(crate) fn handle_new_timestamp(
        &self,
        server_timestamp: u32,
        client_timestamp: Option<i64>,
    ) -> Option<()> {
        if let Some(client_timestamp) = client_timestamp {
            let receive_time = self.client_time();
            let round_trip_time = receive_time.checked_sub(client_timestamp as u32)?;
            let server_time_at_receive = server_timestamp.checked_sub(round_trip_time.div(2))?;

            // Checked sub because if start_time was too long ago, it will overflow and panic
            let offset = server_time_at_receive.checked_sub(receive_time)?;
            *self.server_time_offset.lock() = offset;
        }

        Some(())
    }

    pub(crate) fn new_topic_id(&self) -> u32 {
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
        id: UnsignedIntOrNegativeOne,
        r#type: Type,
        timestamp: u32,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        let mut buf = Vec::<u8>::with_capacity(19);

        // TODO: too lazy to handle these errors ????
        rmp::encode::write_array_len(&mut buf, 4).unwrap();
        // Client side topic is guaranteed to have a uid
        id.write_to_buf(&mut buf).unwrap();
        rmp::encode::write_u32(&mut buf, timestamp as u32).unwrap();
        rmp::encode::write_u32(&mut buf, r#type.as_u8() as u32).unwrap();
        rmpv::encode::write_value(&mut buf, value).unwrap();

        Ok(self.send_message(Message::Binary(buf)).await)
    }

    /// Value should match topic type
    pub(crate) async fn publish_value(
        &self,
        id: UnsignedIntOrNegativeOne,
        r#type: Type,
        value: &rmpv::Value,
    ) -> Result<(), crate::Error> {
        self.publish_value_w_timestamp(id, r#type, self.server_time(), value)
            .await
    }

    pub(crate) async fn update_time(&self) -> Result<(), crate::Error> {
        let announced_topics = self.announced_topics.lock().await;
        let time_topic = announced_topics.get(&-1);

        if let Some(time_topic) = time_topic {
            cfg_tracing! {
                tracing::trace!("Updating timestamp.");
            }

            return self
                .publish_value_w_timestamp(
                    UnsignedIntOrNegativeOne::NegativeOne,
                    time_topic.r#type,
                    0,
                    &rmpv::Value::Integer(self.client_time().into()),
                )
                .await;
        }

        Ok(())
    }

    // Called on connection open, must not fail!
    pub(crate) async fn on_open(&self) {
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

        // Send all messages at once (please don't fail ????)
        self.send_message(Message::Text(serde_json::to_string(&messages).unwrap()))
            .await;

        cfg_tracing! {
            tracing::info!("Prepared new connection.");
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

/// Handles messages from the server
async fn handle_message(client: Arc<InnerClient>, message: Message) {
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

                        // Call user provided on announce fn
                        (client.config.on_announce)(announced.get(&id).unwrap());
                    }
                    NTMessage::UnAnnounce(un_announce) => {
                        cfg_tracing! {
                            tracing::info!("Server un_announced: {}", un_announce.name);
                        }

                        let removed = client.announced_topics.lock().await.remove(&un_announce.id);
                        (client.config.on_un_announce)(removed);
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

            // Put the raw data in a VecDeque because its read impl removes bytes from it
            // so we keep deserializing msgpack from the VecDeque until it is emptied out
            let mut msgpack = VecDeque::from(msgpack);
            while let Ok(data) = rmp_serde::decode::from_read(&mut msgpack) {
                match data {
                    rmpv::Value::Array(array) => handle_value(array, Arc::clone(&client)).await,
                    _ => {
                        cfg_tracing! {
                            tracing::error!("Server sent an invalid msgpack data, not an array.");
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

async fn handle_value(array: Vec<rmpv::Value>, client: Arc<InnerClient>) {
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
                        if let Some(topic) = client.announced_topics.lock().await.get(&(id as i32))
                        {
                            cfg_tracing! {tracing::trace!("Received Value: {topic:?} {type:?} {data:?}");}
                            send_value_to_subscriber(
                                client.clone(),
                                topic,
                                timestamp_micros,
                                r#type,
                                data,
                            )
                            .await;
                        } else {
                            // Topic wasn't previously announced or hasn't been announced yet
                            // Spawn a task to try and add it again
                            // this shouldn't happen anymore, but for safety I'll keep it ????
                            let client = client.clone();
                            let data = data.to_owned();

                            cfg_tracing! {
                                tracing::error!("Received a topic before it was announced! ????");
                            }

                            tokio::spawn(async move {
                                tokio::time::sleep(Duration::from_millis(7)).await;
                                if let Some(topic) =
                                    client.announced_topics.lock().await.get(&(id as i32))
                                {
                                    send_value_to_subscriber(
                                        client.clone(),
                                        topic,
                                        timestamp_micros,
                                        r#type,
                                        &data,
                                    )
                                    .await
                                }
                            });
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
                match client.handle_new_timestamp(timestamp_micros, data.as_i64()) {
                    Some(_) => {}
                    None => {
                        // Math failed, update most recent time
                        *client.start_time.lock() = Instant::now();
                        client.update_time().await.ok();
                        client.handle_new_timestamp(timestamp_micros, data.as_i64());
                    }
                };
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

async fn send_value_to_subscriber(
    client: Arc<InnerClient>,
    topic: &Topic,
    timestamp_micros: u32,
    r#type: Type,
    data: &rmpv::Value,
) {
    // Allows sent values to be handled by subs, cause there hasnt been an await for a while
    yield_now().await;

    client.subscriptions.lock().await.retain(|_, sub| {
        if !sub.is_valid() {
            println!("invalid sub");
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
                true
            }
        }
    });
}

/// Upgrade the weak pointer or stop the task
macro_rules! upgrade_client {
    ($client:expr) => {
        match $client.upgrade() {
            Some(v) => v,
            None => break,
        }
    };
}

async fn setup_socket(
    client: Weak<InnerClient>,
    mut receiver: mpsc::Receiver<Message>,
) -> Result<(), crate::Error> {
    let mut request = format!(
        "ws://{}/nt/rust-client-{}",
        client.upgrade().unwrap().server_addr,
        rand::random::<u32>()
    )
    .into_client_request()?;
    // Add sub-protocol header
    request.headers_mut().append(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_static("networktables.first.wpi.edu"),
    );
    let uri = request.uri().clone();

    let (mut socket, _) = tokio::time::timeout(
        Duration::from_millis(client.upgrade().unwrap().config.connect_timeout),
        tokio_tungstenite::connect_async(request),
    )
    .await??;

    cfg_tracing! {
        tracing::info!("Connected to {}", uri);
    }

    tokio::spawn(async move {
        loop {
            let _: Result<(), crate::Error> = select! {
                message = socket.try_next() => {
                    // Message from server

                    match message {
                        Ok(message) => {
                            cfg_tracing! {tracing::trace!("Received Message");}
                            // unwrap should be fine because the "stream" never ends and error is already handled
                            handle_message(upgrade_client!(client), message.unwrap()).await;
                        },
                        Err(err) => handle_disconnect(Err::<(), _>(err), upgrade_client!(client), &mut socket).await?,
                    };

                    Ok(())
                },
                message = receiver.recv() => {
                    // Message from client
                    if let Some(message) = message {
                        handle_disconnect(
                            socket.send(message).await,
                            upgrade_client!(client),
                            &mut socket
                        ).await?;
                    } else {
                        // Other side of channel was dropped, end task
                        break;
                    }

                    Ok(())
                },
            };
        }

        Ok::<(), crate::Error>(())
    });

    Ok(())
}

async fn handle_disconnect<T>(
    result: Result<T, tokio_tungstenite::tungstenite::Error>,
    client: Arc<InnerClient>,
    socket: &mut WebSocket,
) -> Result<(), tokio_tungstenite::tungstenite::Error> {
    match result {
        Ok(_) => Ok(()),
        Err(err) => match err {
            tokio_tungstenite::tungstenite::Error::AlreadyClosed
            | tokio_tungstenite::tungstenite::Error::ConnectionClosed => {
                cfg_tracing! {
                    tracing::info!("Disconnected from server, attempting to reconnect.");
                }
                (client.config.on_disconnect)();
                loop {
                    tokio::time::sleep(Duration::from_millis(client.config.connect_timeout)).await;

                    let mut request = format!("ws://{}/nt/rust-client", client.server_addr)
                        .into_client_request()
                        .unwrap();
                    // Add sub-protocol header
                    request.headers_mut().append(
                        "Sec-WebSocket-Protocol",
                        HeaderValue::from_static("networktables.first.wpi.edu"),
                    );

                    match tokio::time::timeout(
                        Duration::from_millis(client.config.connect_timeout),
                        tokio_tungstenite::connect_async(request),
                    )
                    .await
                    {
                        Ok(connect_result) => match connect_result {
                            Ok((new_socket, _)) => {
                                *socket = new_socket;
                                client.on_open().await;
                                (client.config.on_reconnect)();

                                cfg_tracing! {
                                    tracing::info!("Successfully reestablished connection.");
                                }

                                break Ok(());
                            }
                            Err(_) => {}
                        },
                        Err(_) => {}
                    }
                }
            }
            _ => Err(err),
        },
    }
}

#[derive(Debug)]
enum UnsignedIntOrNegativeOne {
    NegativeOne,
    UnsignedInt(u32),
}

impl UnsignedIntOrNegativeOne {
    pub fn write_to_buf<W: rmp::encode::RmpWrite>(
        &self,
        wr: &mut W,
    ) -> Result<(), rmp::encode::ValueWriteError<W::Error>> {
        match self {
            Self::NegativeOne => rmp::encode::write_i32(wr, -1),
            Self::UnsignedInt(u_int) => rmp::encode::write_u32(wr, *u_int),
        }
    }
}
