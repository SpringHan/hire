//! The app-wide error types and the `rt_error!` shorthand.

mod types;

pub use types::*;

/// Return a message as an `AppError`: the counterpart of anyhow's `bail!` for
/// hire's own result type.
#[macro_export]
macro_rules! rt_error {
    ($x: expr) => {
        return Err(anyhow::anyhow!($x).into())
    };
}
