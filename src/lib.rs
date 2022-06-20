#[macro_use]
extern crate anyhow;
use anyhow::Result;
use bb8_redis::{bb8::Pool, RedisConnectionManager};

pub mod client;
pub mod protocol;
pub mod server;

pub use crate::protocol::Error;
pub use crate::protocol::ObjectID;

pub async fn pool<S: AsRef<str>>(url: S) -> Result<Pool<RedisConnectionManager>> {
    let mgr = RedisConnectionManager::new(url.as_ref())?;
    Ok(Pool::builder().max_size(20).build(mgr).await?)
}
