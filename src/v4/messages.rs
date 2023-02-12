use std::{borrow::Cow, collections::HashSet};

use serde::{Deserialize, Serialize};

use super::{subscription::SubscriptionOptions, topic::PublishProperties, Type};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "lowercase")]
pub(crate) enum NTMessage<'a> {
    #[serde(borrow = "'a")]
    Publish(PublishTopic<'a>),
    Unpublish(UnpublishTopic),
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    SetProperties(SetProperties<'a>),
    #[serde(borrow = "'a")]
    Announce(Announce<'a>),
    #[serde(borrow = "'a")]
    UnAnnounce(UnAnnounce<'a>),
    #[serde(borrow = "'a")]
    Properties(Properties<'a>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct PublishTopic<'a> {
    pub(crate) name: &'a str,
    pub(crate) pubuid: u32,
    pub(crate) r#type: Type,
    /// Initial topic properties.
    /// If the topic is newly created (e.g. there are no other publishers) this sets the topic properties.
    /// If the topic was previously published, this is ignored. The announce message contains the actual topic properties.
    /// Clients can use the setproperties message to change properties after topic creation.
    #[serde(borrow = "'a", skip_serializing_if = "Option::is_none")]
    pub(crate) properties: Cow<'a, Option<PublishProperties>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct UnpublishTopic {
    pub(crate) pubuid: u32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Subscribe {
    pub(crate) subuid: i32,
    pub(crate) topics: HashSet<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) options: Option<SubscriptionOptions>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Unsubscribe {
    pub(crate) subuid: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct SetProperties<'a> {
    pub(crate) name: &'a str,
    pub(crate) update: Cow<'a, PublishProperties>,
}

/// The server shall send this message for each of the following conditions:
/// - To all clients subscribed to a matching prefix when a topic is created
/// - To a client in response to an Publish Request Message (publish) from that client
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Announce<'a> {
    /// Topic name
    pub(crate) name: &'a str,
    /// Topic id
    pub(crate) id: i32,
    /// Topic type
    pub(crate) r#type: Type,
    /// If this message was sent in response to a publish message,
    /// the Publisher UID provided in that message. Otherwise absent.
    pub(crate) pubuid: Option<i32>,
    /// Topic properties
    pub(crate) properties: PublishProperties,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct UnAnnounce<'a> {
    /// Topic name
    pub(crate) name: &'a str,
    /// Topic id
    pub(crate) id: i32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub struct Properties<'a> {
    /// Topic name
    pub(crate) name: &'a str,
    /// Acknowledgement - True if this message is in response to a setproperties message from the same client.
    /// Otherwise absent.
    pub(crate) ack: Option<bool>,
}
