use crate::{
    protocol::{Error, ObjectID, Output, Request, Response, Result},
    server::{stream_receiverx, ReceiverX},
};
use bb8_redis::{bb8::Pool, redis::AsyncCommands, RedisConnectionManager};
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use tokio::time::{sleep, Duration};
/// raw rbus client object.
/// Usually you would wrap this client in a stub to use more
/// abstract functions.
#[derive(Clone)]
pub struct Client {
    pool: Pool<RedisConnectionManager>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    data: String,
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

    pub async fn stream<S, T>(
        &mut self,
        module: S,
        object_id: ObjectID,
        key: String,
    ) -> Result<ReceiverX<T>>
    where
        S: AsRef<str>,
    {
        let (sender, receiver) = stream_receiverx();
        let channel = format!("{}.{}.{}", module.as_ref(), object_id, key);

        let client = bb8_redis::redis::Client::open("redis://127.0.0.1/")
            .map_err(|err| Error::Protocol(format!("failed to get redis client: {}", err)))?;

        let mut con = client
            .get_connection()
            .map_err(|err| Error::Protocol(format!("failed to get redis connection: {}", err)))?;
        tokio::spawn(async move {
            let mut pubsub = con.as_pubsub();
            loop {
                if let Err(err) = pubsub.subscribe(&channel) {
                    log::error!(
                        "Failed to subscribe to channel: {}. Error was {}",
                        &channel,
                        err
                    );
                }
                sleep(Duration::from_secs(2)).await;
                let msg = match pubsub.get_message() {
                    Ok(msg) => msg,
                    Err(err) => {
                        log::error!("failed to get message. Error was {}", err);
                        continue;
                    }
                };
                let message: Vec<u8> = match msg.get_payload() {
                    Ok(message) => message,
                    Err(err) => {
                        log::error!("failed to get message payload. Error was {}", err);
                        continue;
                    }
                };
                match sender.send(ByteBuf::from(message)).await {
                    Ok(_) => (),
                    Err(err) => {
                        log::error!(
                            "Can not send retrived message from redis channel. Error was {}",
                            err
                        );
                        continue;
                    }
                };
            }
        });
        Ok(receiver)
    }
    pub async fn stream_test<T>(mut self) -> ReceiverX<T> {
        let receiver = self
            .stream(
                "full-test",
                ObjectID::new("calculator", "1.0"),
                "stream_test".to_string(),
            )
            .await
            .unwrap();
        receiver
    }
}
