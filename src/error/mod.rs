mod types;

pub use types::*;

/// The `bail!` macro for hire crate.
#[macro_export]
macro_rules! rt_error {
    ($x: expr) => {
        return Err(anyhow::anyhow!($x).into())
    };
}
