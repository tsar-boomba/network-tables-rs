#[macro_use]
pub mod macros;
pub mod error;

pub use error::Error;

#[cfg(feature = "__v3")]
pub mod v3;

#[cfg(feature = "__v4")]
pub mod v4;
#[cfg(feature = "__v4")]
pub use rmpv::{self, Value};

#[inline(always)]
fn log_result<T, E: std::error::Error>(result: Result<T, E>) -> Result<T, E> {
    #[cfg(feature = "tracing")]
    match &result {
        Err(err) => {
            tracing::error!("{}", err)
        }
        _ => {}
    };
    result
}
