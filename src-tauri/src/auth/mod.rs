//! Authentication module

pub mod oauth_server;
pub(crate) mod persistence;
pub mod storage;
pub mod switcher;
pub mod token_refresh;

#[cfg(test)]
pub(crate) mod test_support;

// Token rotation and account switching must never overlap, including network requests.
pub(crate) static AUTH_OPERATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub use oauth_server::*;
pub use storage::*;
pub use switcher::*;
pub use token_refresh::*;
