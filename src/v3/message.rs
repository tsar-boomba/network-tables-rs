use bytes::Bytes;

/// [read more](https://github.com/wpilibsuite/allwpilib/blob/main/ntcore/doc/networktables3.adoc#entry-types)
#[repr(u16)]
#[derive(Debug, Clone)]
pub enum Type {
	Boolean = 0x00,
	Double = 0x01,
	String = 0x02,
	Raw = 0x03,
	BooleanArray = 0x10,
	DoubleArray = 0x11,
	StringArray = 0x12,
	RPCDefinition = 0x29
}


#[derive(Debug, Clone)]
pub enum EntryData<'a> {
    Boolean(bool),
    Double(f64),
    String(&'a str),
    Raw(&'a [u8]),
    BooleanArray(&'a [bool]),
    DoubleArray(&'a [f64]),
    StringArray(&'a [&'a str]),
}

#[derive(Debug, Clone)]
pub(crate) enum Message<'a> {
    KeepAlive,
    ClientHello {
        protocol_revision: u16,
        name: &'a str,
    },
    ProtocolVersionUnsupported {
        supported_protocol_revision: u16,
    },
    ServerHelloComplete,
    ServerHello {
        flags: u8,
        name: &'a str,
    },
    ClientHelloComplete,
    EntryAssignment {
        name: &'a str,
        r#type: Type,
        id: u16,
        sequence_number: u16,
        flags: u8,
        value: EntryData<'a>,
    },
    EntryUpdate {
		id: u16,
        sequence_number: u16,
        r#type: Type,
        value: EntryData<'a>,
    },
    EntryFlagsUpdate {
		id: u16,
		flags: u8,
	},
    EntryDelete {
		id: u16
	},
    ClearAllEntries {

	},
	/// Not supported
    RPCExecute,
	/// Not supported
    RPCResponse,
}

impl<'a> Message<'a> {
    pub fn message_id(&self) -> u8 {
        match self {
            Self::KeepAlive => 0x00,
            Self::ClientHello { .. } => 0x01,
            Self::ProtocolVersionUnsupported { .. } => 0x02,
            Self::ServerHelloComplete => 0x03,
            Self::ServerHello { .. } => 0x04,
            Self::ClientHelloComplete => 0x05,
            Self::EntryAssignment { .. } => 0x10,
            Self::EntryUpdate { .. } => 0x11,
            Self::EntryFlagsUpdate { .. } => 0x12,
            Self::EntryDelete { .. } => 0x13,
            Self::ClearAllEntries { .. } => 0x14,
            Self::RPCExecute => 0x20,
            Self::RPCResponse => 0x21,
        }
    }

	pub fn from_bytes(bytes: Bytes) {}

	pub fn as_bytes(&self) -> Bytes {
		todo!()
	}
}
