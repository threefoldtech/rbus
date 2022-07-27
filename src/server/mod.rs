use crate::protocol::{self, Error, ObjectID, Output, Request, Result, Tuple};
use async_trait::async_trait;
use rmp_serde;
use serde::{de::DeserializeOwned, Serialize};
use serde_bytes::ByteBuf;
use std::collections::HashMap;
use std::marker::PhantomData;
use thiserror::Error;
use tokio::sync::mpsc;
pub mod redis;
pub use self::redis::Server;
pub struct SenderX<T> {
    tx: mpsc::Sender<serde_bytes::ByteBuf>,
    p: PhantomData<T>,
}
impl<T> SenderX<T>
where
    T: Serialize,
{
    pub async fn send(&self, msg: &T) -> Result<()> {
        let msg = protocol::encode(msg)?;
        match self.tx.send(msg).await {
            Ok(_) => {}
            Err(err) => log::error!("failed to send output to channel: {}", err),
        };
        Ok(())
    }
}
pub struct Sender {
    tx: mpsc::Sender<serde_bytes::ByteBuf>,
}
impl Sender {
    pub async fn send(&self, msg: ByteBuf) -> Result<()> {
        match self.tx.send(msg).await {
            Ok(_) => {}
            Err(err) => log::error!("failed to send output to channel: {}", err),
        };
        Ok(())
    }
}
pub struct Receiver {
    pub rx: mpsc::Receiver<serde_bytes::ByteBuf>,
}
impl Receiver {
    pub async fn recv(&mut self) -> Option<serde_bytes::ByteBuf> {
        self.rx.recv().await
    }
}

pub struct ReceiverX<T> {
    rx: mpsc::Receiver<serde_bytes::ByteBuf>,
    p: PhantomData<T>,
}
impl<T> ReceiverX<T>
where
    T: DeserializeOwned,
{
    pub async fn recv(&mut self) -> Option<T> {
        let received = self.rx.recv().await?;
        let decoded = rmp_serde::decode::from_read_ref(&received);
        match decoded {
            Ok(decoded) => Some(decoded),
            Err(err) => {
                log::error!("failed to decode message. Error was {}", err);
                None
            }
        }
    }
}
pub fn stream_senderx<T>() -> (SenderX<T>, Receiver) {
    let (tx, rx) = mpsc::channel(5);
    return (SenderX { tx, p: PhantomData }, Receiver { rx });
}
pub fn stream_receiverx<T>() -> (Sender, ReceiverX<T>) {
    let (tx, rx) = mpsc::channel(5);
    return (Sender { tx }, ReceiverX { rx, p: PhantomData });
}
/// Object trait
#[async_trait]
pub trait Object {
    /// return object id
    fn id(&self) -> ObjectID;
    fn streams(&self) -> Result<HashMap<String, Receiver>>;

    /// dispatch request and get an Output
    async fn dispatch(&self, request: Request) -> Result<Output>;
}

/// Handlers must implement this trait
#[async_trait]
pub trait Handler {
    async fn handle(&self, a: Tuple) -> Result<Output>;
}

/// SimpleObject implements Object trait
/// usually you would never need to use this. Instead use
/// the `object` macro on a trait to generate Dispatchers and Stubs
/// for that interface.
pub struct SimpleObject {
    o: ObjectID,
    handlers: HashMap<String, Box<dyn Handler + Sync + Send>>,
}

impl SimpleObject {
    pub fn new(id: ObjectID) -> SimpleObject {
        SimpleObject {
            o: id,
            handlers: HashMap::new(),
        }
    }

    pub fn handle<S, H>(mut self, m: S, h: H) -> SimpleObject
    where
        S: Into<String>,
        H: Handler + Send + Sync + 'static,
    {
        self.handlers.insert(m.into(), Box::new(h));
        self
    }
}

#[async_trait]
impl Object for SimpleObject {
    fn id(&self) -> ObjectID {
        self.o.clone()
    }

    async fn dispatch(&self, request: Request) -> Result<Output> {
        let handler = match self.handlers.get(&request.method) {
            Some(handler) => handler,
            None => return Err(Error::UnknownMethod(request.method)),
        };

        handler.handle(request.inputs).await
    }
    fn streams(&self) -> Result<HashMap<String, Receiver>> {
        let mut streams_map = HashMap::new();
        let (_, rx) = mpsc::channel(1);
        streams_map.insert("".to_string(), Receiver { rx });
        Ok(streams_map)
    }
}
