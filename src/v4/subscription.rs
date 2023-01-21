use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Weak},
};

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{
    messages::{NTMessage, Unsubscribe},
    Topic, Type,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    pub topic_name: String,
    pub timestamp: u32,
    pub r#type: Type,
    pub data: rmpv::Value,
}

#[derive(Debug, Clone)]
pub struct SubscriptionData {
    pub(crate) subuid: i32,
    pub(crate) topics: HashSet<String>,
    pub(crate) options: Option<SubscriptionOptions>,
}

#[derive(Debug)]
pub struct InternalSub {
    pub(crate) data: Weak<SubscriptionData>,
    pub(crate) sender: mpsc::Sender<MessageData>,
}

#[derive(Debug)]
pub struct Subscription {
    pub(crate) data: Arc<SubscriptionData>,
    pub(crate) receiver: mpsc::Receiver<MessageData>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct SubscriptionOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub periodic: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topics_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<bool>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub rest: Option<HashMap<String, serde_json::Value>>,
}

impl InternalSub {
    pub(crate) fn is_valid(&self) -> bool {
        self.data.strong_count() != 0
    }

    pub(crate) fn matches_topic(&self, topic: &Topic) -> bool {
        if let Some(data) = self.data.upgrade() {
            let prefix = data
                .options
                .as_ref()
                .and_then(|options| options.prefix)
                .unwrap_or(false);

            if prefix {
                data.topics
                    .iter()
                    .any(|topic_pat| topic.name.starts_with(topic_pat))
            } else {
                data.topics
                    .iter()
                    .any(|topic_name| *topic_name == topic.name)
            }
        } else {
            false
        }
    }
}

impl Subscription {
    pub(crate) fn as_unsubscribe(&self) -> NTMessage {
        NTMessage::Unsubscribe(Unsubscribe {
            subuid: self.data.subuid,
        })
    }

    pub async fn next(&mut self) -> Option<MessageData> {
        self.receiver.recv().await
    }

    pub fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<MessageData>> {
        self.receiver.poll_recv(cx)
    }
}

impl Stream for Subscription {
    type Item = MessageData;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.poll_next(cx)
    }
}
