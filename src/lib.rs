#[macro_use]
pub mod macros;
pub mod error;

pub use error::Error;

#[cfg(any(feature = "client-v4"))]
pub mod v4;
#[cfg(any(feature = "client-v4"))]
pub use rmpv::{self, Value};
