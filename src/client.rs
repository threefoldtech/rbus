use crate::protocol::{Error, ObjectID, Output, Request, Response, Result};
use anyhow::Context;
use bb8_redis::{
    bb8::Pool,
    redis::{AsyncCommands, ConnectionInfo, IntoConnectionInfo},
    RedisConnectionManager,
};
use serde::de::DeserializeOwned;
use serde_bytes::ByteBuf;
use std::marker::PhantomData;
use tokio::sync::mpsc;

/// Receiver is returned by the stream method of the client. Used to subscribe to events.
pub struct Receiver<T> {
    rx: mpsc::Receiver<serde_bytes::ByteBuf>,
    p: PhantomData<T>,
}
impl<T> Receiver<T>
where
    T: DeserializeOwned,
{
    fn new() -> (Self, Source) {
        let (tx, rx) = mpsc::channel(5);
        (Self { rx, p: PhantomData }, Source { tx })
    }

    /// recv receives an event. return None of subscription was stopped (lost redis connection, etc..)
    /// it's up to the caller to retry subscribing to the event stream again.
    pub async fn recv(&mut self) -> Option<anyhow::Result<T>> {
        let received = self.rx.recv().await?;
        //I really think this should be unwrap because it means there is a "logic" error
        // not just runtime error
        Some(rmp_serde::decode::from_read_ref(&received).map_err(|err| anyhow::anyhow!("{}", err)))
    }
}

/// Source is used internally by the client to send received bytes to Receiver
struct Source {
    tx: mpsc::Sender<serde_bytes::ByteBuf>,
}

impl Source {
    /// send bytes to receiver
    #[allow(dead_code)]
    pub async fn send(&self, msg: ByteBuf) -> anyhow::Result<()> {
        Ok(self.tx.send(msg).await?)
    }

    /// send_blocking is a work around for the unstable async redis pubsub. instead we do subscribe
    /// in blocking code (thread)
    pub fn send_blocking(&self, msg: ByteBuf) -> anyhow::Result<()> {
        Ok(self.tx.blocking_send(msg)?)
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

    fn subscribe<C: AsRef<str>>(info: ConnectionInfo, source: Source, ch: C) -> anyhow::Result<()> {
        let client = bb8_redis::redis::Client::open(info)?;
        let mut con = client.get_connection()?;
        let mut pubsub = con.as_pubsub();
        pubsub
            .subscribe(ch.as_ref())
            .context("failed to subscribe to events")?;

        loop {
            let msg = pubsub.get_message().context("failed to get events")?;
            let payload: Vec<u8> = msg.get_payload()?;
            // if source failed to send it means there is no receiver anymore
            if source.send_blocking(ByteBuf::from(payload)).is_err() {
                // if we send to push the received message then the receiver has exited
                // hence we just return and never try again.
                return Ok(());
            }
        }
    }

    /// low level stream, registers to an even stream from the given module/object with given stream name. T must match the
    /// event type sent by the server (even source).
    pub async fn stream<S, T, K>(&self, module: S, object: ObjectID, key: K) -> Result<Receiver<T>>
    where
        S: AsRef<str>,
        K: AsRef<str>,
        T: DeserializeOwned,
    {
        let (receiver, source) = Receiver::new();
        let channel = format!("{}.{}.{}", module.as_ref(), object, key.as_ref());

        let info = self.info.clone();
        // unfortunately the async redis pubsub is not stable and is causing issues
        // hence we instead, starting a blocking thread to subscribe.
        tokio::task::spawn_blocking(move || {
            if let Err(err) = Self::subscribe(info, source, &channel) {
                log::error!(
                    "subscription to events channel '{}' stopped: {}",
                    channel,
                    err,
                )
            }
        });

        Ok(receiver)
    }
}
