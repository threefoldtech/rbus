use crate::protocol::{Arguments, Request, Response};
use anyhow::{Context, Result};
use bb8_redis::{bb8::Pool, RedisConnectionManager};
use redis::AsyncCommands;

#[derive(Clone)]
pub struct Client {
    pool: Pool<RedisConnectionManager>,
}

impl Client {
    pub fn new<S>(pool: Pool<RedisConnectionManager>) -> Client {
        Self { pool }
    }

    // a new version with cancellation context need to be implemented
    pub async fn request<S>(&mut self, module: S, request: Request) -> Result<Arguments>
    where
        S: AsRef<str>,
    {
        let mut con = self
            .pool
            .get()
            .await
            .context("failed to get redis connection")?;

        let queue = format!("{}.{}", module.as_ref(), request.object);

        con.rpush(queue, &request)
            .await
            .context("failed to send request")?;

        // wait for response
        // todo: timeout on response
        let (_, response): (String, Response) = con
            .blpop(&request.id, 0)
            .await
            .context("failed t get response")?;

        if response.is_error() {
            bail!("zbus error: {}", response.error.unwrap());
        }

        Ok(response.arguments)
    }
}
