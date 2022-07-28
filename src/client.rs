use crate::protocol::{Error, ObjectID, Output, Request, Response, Result};
use anyhow::Context;
use bb8_redis::{
    bb8::Pool,
    redis::{AsyncCommands, ConnectionInfo, IntoConnectionInfo},
    RedisConnectionManager,
};
use futures_util::StreamExt;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::marker::PhantomData;
use tokio::sync::mpsc;

pub struct Receiver<T> {
    rx: mpsc::Receiver<serde_bytes::ByteBuf>,
    p: PhantomData<T>,
}
impl<T> Receiver<T>
where
    T: DeserializeOwned,
{
    pub fn new() -> (Self, Source) {
        let (tx, rx) = mpsc::channel(5);
        return (Self { rx, p: PhantomData }, Source { tx });
    }

    pub async fn recv(&mut self) -> Option<anyhow::Result<T>> {
        let received = self.rx.recv().await?;
        //I really think this should be unwrap because it means there is a "logic" error
        // not just runtime error
        Some(rmp_serde::decode::from_read_ref(&received).map_err(|err| anyhow::anyhow!("{}", err)))
    }
}

pub struct Source {
    tx: mpsc::Sender<serde_bytes::ByteBuf>,
}

impl Source {
    pub async fn send(&self, msg: ByteBuf) -> anyhow::Result<()> {
        Ok(self.tx.send(msg).await?)
    }
}

/// raw rbus client object.
/// Usually you would wrap this client in a stub to use more
/// abstract functions.
#[derive(Clone)]
pub struct Client {
    info: ConnectionInfo,
    pool: Pool<RedisConnectionManager>,
}

impl Client {
    pub async fn new<I: IntoConnectionInfo>(info: I) -> anyhow::Result<Client> {
        let info = info.into_connection_info()?;
        let mgr = RedisConnectionManager::new(info.clone())?;
        let pool = Pool::builder()
            .max_size(super::POOL_SIZE)
            .build(mgr)
            .await?;

        Ok(Self { info, pool })
    }

    /// make a request, and wait for response Output
    /// TODO: a request function with deadline.
    pub async fn request<S>(&self, module: S, request: Request) -> Result<Output>
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

    async fn subscribe(
        pool: Pool<RedisConnectionManager>,
        source: Source,
        ch: String,
    ) -> anyhow::Result<()> {
        let con = pool.dedicated_connection().await?;
        let mut pubsub = con.into_pubsub();
        log::debug!("subscribing to: {}", ch);
        pubsub
            .subscribe(&ch)
            .await
            .context("failed to subscribe to events")?;

        log::debug!("subscribed to: {}", ch);
        let mut messages = pubsub.on_message();

        while let Some(msg) = messages.next().await {
            log::debug!("received event from server");
            let payload: Vec<u8> = msg.get_payload()?;
            // if source failed to send it means there is no receiver anymore
            source.send(ByteBuf::from(payload)).await?;
        }

        Ok(())
    }

    pub async fn stream<S, T, K>(&self, module: S, object: ObjectID, key: K) -> Result<Receiver<T>>
    where
        S: AsRef<str>,
        K: AsRef<str>,
        T: DeserializeOwned,
    {
        let (receiver, source) = Receiver::new();
        let channel = format!("{}.{}.{}", module.as_ref(), object, key.as_ref());

        tokio::task::spawn(Self::subscribe(self.pool.clone(), source, channel));

        Ok(receiver)
    }
}
