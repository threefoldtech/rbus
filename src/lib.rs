use anyhow::Result;
use bb8_redis::{bb8::Pool, RedisConnectionManager};

pub mod client;
pub mod protocol;
pub mod server;

pub use client::Client;
pub use server::Server;

pub async fn pool<S: AsRef<str>>(url: S) -> Result<Pool<RedisConnectionManager>> {
    let mgr = RedisConnectionManager::new(url.as_ref())?;
    Ok(Pool::builder().max_size(20).build(mgr).await?)
}

#[cfg(feature = "macros")]
pub use macros::interface;
