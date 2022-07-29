use crate::protocol::{self, Error, ObjectID, Output, Request, Result, Tuple};
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::marker::PhantomData;
use thiserror::Error;
use tokio::sync::mpsc;
pub mod redis;
pub use self::redis::Server;

/// Sender is used by streams to publish events.
pub struct Sender<T> {
    tx: mpsc::Sender<serde_bytes::ByteBuf>,
    p: PhantomData<T>,
}

impl<T> Sender<T>
where
    T: Serialize,
{
    /// create a new Sender, Sink pair. a
    pub fn new() -> (Self, Sink) {
        let (tx, rx) = mpsc::channel(5);
        (Self { tx, p: PhantomData }, Sink { rx })
    }

    /// send pushed object T as event
    pub async fn send(&self, msg: &T) -> anyhow::Result<()> {
        let msg = protocol::encode(msg)?;
        Ok(self.tx.send(msg).await?)
    }
}

/// Sink is the receiver part of a event Sender. used internally by rbus
pub struct Sink {
    pub rx: mpsc::Receiver<serde_bytes::ByteBuf>,
}

impl Sink {
    async fn recv(&mut self) -> Option<serde_bytes::ByteBuf> {
        self.rx.recv().await
    }
}

/// Object trait
#[async_trait]
pub trait Object {
    /// return object id
    fn id(&self) -> ObjectID;
    fn streams(&self) -> Result<HashMap<String, Sink>>;

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

    fn streams(&self) -> Result<HashMap<String, Sink>> {
        Ok(HashMap::default())
    }
}
