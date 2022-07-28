use anyhow::Result;
use bb8_redis::{bb8::Pool, redis::IntoConnectionInfo, RedisConnectionManager};

pub mod client;
pub mod protocol;
pub mod server;

pub use client::Client;
pub use server::Server;

const POOL_SIZE: u32 = 20;

/// create a redis connection pool that can be used by both client and server
pub async fn pool<S: AsRef<str>>(url: S) -> Result<Pool<RedisConnectionManager>> {
    let mgr = RedisConnectionManager::new(url.as_ref())?;
    Ok(Pool::builder().max_size(POOL_SIZE).build(mgr).await?)
}

#[cfg(feature = "macros")]
pub use macros::object;
