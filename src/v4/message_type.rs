use serde::{de::Visitor, Serialize};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    Boolean,
    Double,
    Int,
    Float,
    String,
    Json,
    Raw,
    Rpc,
    MsgPack,
    ProtoBuf,
    #[serde(rename = "boolean[]")]
    BooleanArray,
    #[serde(rename = "double[]")]
    DoubleArray,
    #[serde(rename = "int[]")]
    IntArray,
    #[serde(rename = "float[]")]
    FloatArray,
    #[serde(rename = "string[]")]
    StringArray,
}

impl Type {
    #[inline]
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Boolean => 0,
            Self::Double => 1,
            Self::Int => 2,
            Self::Float => 3,
            Self::String => 4,
            Self::Json => 4,
            Self::Raw => 5,
            Self::Rpc => 5,
            Self::MsgPack => 5,
            Self::ProtoBuf => 5,
            Self::BooleanArray => 16,
            Self::DoubleArray => 17,
            Self::IntArray => 18,
            Self::FloatArray => 19,
            Self::StringArray => 20,
        }
    }

    #[inline]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Double => "double",
            Self::Int => "int",
            Self::Float => "float",
            Self::String => "string",
            Self::Json => "json",
            Self::Raw => "raw",
            Self::Rpc => "rpc",
            Self::MsgPack => "msgpack",
            Self::ProtoBuf => "protobuf",
            Self::BooleanArray => "boolean[]",
            Self::DoubleArray => "double[]",
            Self::IntArray => "int[]",
            Self::FloatArray => "float[]",
            Self::StringArray => "string[]",
        }
    }

    #[inline]
    pub fn from_num(num: u64) -> Option<Self> {
        match num {
            0 => Some(Self::Boolean),
            1 => Some(Self::Double),
            2 => Some(Self::Int),
            3 => Some(Self::Float),
            4 => Some(Self::String),
            5 => Some(Self::Raw),
            16 => Some(Self::BooleanArray),
            17 => Some(Self::DoubleArray),
            18 => Some(Self::IntArray),
            19 => Some(Self::FloatArray),
            20 => Some(Self::StringArray),
            _ => None,
        }
    }

    #[inline]
    pub fn from_str(str: impl AsRef<str>) -> Option<Self> {
        match str.as_ref() {
            "boolean" => Some(Self::Boolean),
            "double" => Some(Self::Double),
            "int" => Some(Self::Int),
            "float" => Some(Self::Float),
            "string" => Some(Self::String),
            "json" => Some(Self::Json),
            "raw" => Some(Self::Raw),
            "rpc" => Some(Self::Rpc),
            "msgpack" => Some(Self::MsgPack),
            "protobuf" => Some(Self::ProtoBuf),
            "boolean[]" => Some(Self::BooleanArray),
            "double[]" => Some(Self::DoubleArray),
            "int[]" => Some(Self::IntArray),
            "float[]" => Some(Self::FloatArray),
            "string[]" => Some(Self::StringArray),
            _ => None,
        }
    }
}

struct MessageTypeVisitor;

impl<'de> Visitor<'de> for MessageTypeVisitor {
    type Value = Type;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "A valid network tables 4 type string.")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Type::from_str(v).ok_or(E::custom("Not a valid network tables 4 type string."))
    }
}

impl<'de> serde::de::Deserialize<'de> for Type {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(MessageTypeVisitor)
    }
}
