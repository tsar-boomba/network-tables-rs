#[cfg(feature = "client-v3")]
pub mod client;
#[cfg(feature = "client-v3")]
pub mod client_config;

pub mod entry;
pub mod message;

use std::collections::VecDeque;

pub use entry::*;
pub use message::*;


////////////////
/// LEB128 encoding & decoding functions
////////////////

trait FromSlice
where
    Self: Sized,
{
    fn from_slice(slice: &[u8]) -> Result<Self, crate::Error>;
}

impl FromSlice for String {
    fn from_slice(slice: &[u8]) -> Result<Self, crate::Error> {
        String::from_utf8(slice.to_vec()).map_err(|e| e.into())
    }
}

impl FromSlice for Vec<u8> {
	fn from_slice(slice: &[u8]) -> Result<Self, crate::Error> {
		Ok(Vec::from(slice))
	}
}

/// Encodes a slice according to networktables v3 specification.
/// This is useful for encoding strings and raw data for communication
fn leb_128_encode_bytes(slice: &[u8]) -> Result<Vec<u8>, crate::Error> {
    // TODO: Somehow find length ahead of encoding length to can use an array
    let mut buf: Vec<u8> = Vec::with_capacity(slice.len());
    leb128::write::unsigned(&mut buf, slice.len() as u64)?;
    buf.extend(slice);
    Ok(buf)
}

fn leb128_decode_bytes<T: FromSlice>(buf: Vec<u8>) -> Result<T, crate::Error> {
	let mut buf = VecDeque::from(buf);
	// Make into a single slice
	buf.make_contiguous();

    // VecDeque Read impl removes the bytes that were read
    // leaving the bytes we actually care about
    let _ = leb128::read::unsigned(&mut buf)?;
    T::from_slice(buf.as_slices().0)
}
