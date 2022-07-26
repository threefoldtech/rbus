use crate::{
    protocol::{Error, ObjectID, Output, Request, Response, Result},
    server::{stream, Receiver},
};
use bb8_redis::{bb8::Pool, redis::AsyncCommands, RedisConnectionManager};
use futures_util::stream::StreamExt;
use serde_bytes::ByteBuf;
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
    pub async fn stream<S>(
        &mut self,
        module: S,
        object_id: ObjectID,
        key: String,
    ) -> Result<Receiver>
    where
        S: AsRef<str>,
    {
        let (sender, receiver) = stream();
        let pool = self.pool.clone();
        let channel = format!("{}.{}.{}", module.as_ref(), object_id, key);
        tokio::spawn(async move {
            let con = pool
                .dedicated_connection()
                .await
                .map_err(|err| Error::Protocol(format!("failed to get redis connection: {}", err)))
                .unwrap();
            let mut pubsub = con.into_pubsub();
            pubsub.subscribe(channel).await.unwrap();
            let mut pubsub_stream = pubsub.on_message();
            while let Some(msg) = pubsub_stream.next().await {
                let message: Vec<u8> = msg.get_payload().unwrap();
                println!("message {:?}", message);
                let message = ByteBuf::from(message);
                match sender.send(message).await {
                    Ok(_) => {
                        println!("message send")
                    }
                    Err(_) => {
                        log::error!("Can not send retrived message from redis channel");
                        println!("can not send retrieved message ");
                    }
                };
            }
        });
        Ok(receiver)
    }
    pub async fn stream_test(mut self) -> Receiver {
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
