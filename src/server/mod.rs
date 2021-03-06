use crate::protocol::{Error, ObjectID, Output, Request, Result, Tuple};
use async_trait::async_trait;
use std::collections::HashMap;
use thiserror::Error;

pub mod redis;
pub use self::redis::Server;

/// Object trait
#[async_trait]
pub trait Object {
    /// return object id
    fn id(&self) -> ObjectID;
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
}
