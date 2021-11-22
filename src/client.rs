use anyhow::{Context, Result};
use redis::{aio::ConnectionManager, Client as Redis};

use crate::request::{Arguments, Request, Response};

pub use crate::request::Error;

#[derive(Clone)]
pub struct Client {
    redis: ConnectionManager,
}

impl Client {
    pub async fn new<S>(url: S) -> Result<Client>
    where
        S: AsRef<str>,
    {
        let client = Redis::open(url.as_ref())?;
        let redis = client
            .get_tokio_connection_manager()
            .await
            .context("failed to open connection to broker")?;

        Ok(Client { redis })
    }

    // a new version with cancellation context need to be implemented
    pub async fn request<S>(&mut self, module: S, request: Request) -> Result<Arguments>
    where
        S: AsRef<str>,
    {
        let queue = format!("{}.{}", module.as_ref(), request.object);

        redis::cmd("RPUSH")
            .arg(queue)
            .arg(request.encode().context("failed to encode request")?)
            .query_async(&mut self.redis)
            .await?;

        // wait for response
        let (_, result): (String, Vec<u8>) = redis::cmd("BLPOP")
            .arg(request.id)
            .arg(0)
            .query_async(&mut self.redis)
            .await?;

        let response =
            Response::from_slice(result.as_slice()).context("failed to load response")?;

        if response.is_error() {
            bail!("zbus error: {}", response.error.unwrap());
        }

        Ok(response.arguments)
    }
}
