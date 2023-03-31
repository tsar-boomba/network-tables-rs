use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    messages::{NTMessage, UnpublishTopic},
    Type,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct PublishedTopic {
    pub(crate) name: String,
    pub(crate) pubuid: u32,
    pub(crate) r#type: Type,
    pub(crate) properties: Option<PublishProperties>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct Topic {
    pub name: String,
    pub id: i32,
    pub pubuid: Option<i32>,
    pub r#type: Type,
    pub properties: Option<PublishProperties>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub struct PublishProperties {
    /// If true, the last set value will be periodically saved to persistent storage on the server and be restored during server startup.
    /// Topics with this property set to true will not be deleted by the server when the last publisher stops publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistent: Option<bool>,
    /// Topics with this property set to true will not be deleted by the server when the last publisher stops publishing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retained: Option<bool>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub rest: Option<HashMap<String, serde_json::Value>>,
}

impl PublishedTopic {
    pub(crate) fn as_unpublish(&self) -> NTMessage {
        NTMessage::Unpublish(UnpublishTopic {
            pubuid: self.pubuid,
        })
    }
}
