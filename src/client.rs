use crate::protocol::{Error, Output, Request, Response, Result};
use bb8_redis::{bb8::Pool, redis::AsyncCommands, RedisConnectionManager};

/// raw rbus client object.
/// Usually you would wrap this client in a stub to use more
/// abstract functions.
#[derive(Clone)]
pub struct Client {
    pool: Pool<RedisConnectionManager>,
}

impl Client {
    /// create a new instance of the client
    pub fn new(pool: Pool<RedisConnectionManager>) -> Client {
        Self { pool }
    }

    /// make a request, and wait for response Output
    /// TODO: a request function with deadline.
    pub async fn request<S>(&mut self, module: S, request: Request) -> Result<Output>
    where
        S: AsRef<str>,
    {
        let mut con =
            self.pool.get().await.map_err(|err| {
                Error::Protocol(format!("failed to get redis connection: {}", err))
            })?;

        let queue = format!("{}.{}", module.as_ref(), request.object);

        con.rpush(queue, &request)
            .await
            .map_err(|err| Error::Protocol(format!("failed to send request: {}", err)))?;

        // wait for response
        // todo: timeout on response
        let (_, response): (String, Response) = con
            .blpop(&request.id, 0)
            .await
            .map_err(|err| Error::Protocol(format!("failed to get response: {}", err)))?;

        if let Some(err) = response.error {
            return Err(Error::Protocol(err));
        }

        Ok(response.output)
    }
}
