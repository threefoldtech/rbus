use crate::protocol::{Error, ObjectID, Output, Request, Result, Tuple};
use async_trait::async_trait;
use std::collections::HashMap;
use thiserror::Error;

pub mod redis;
pub use self::redis::Server;

/// Server errors
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("unknown method '{0}'")]
    UnknownMethod(String),
}

#[async_trait]
pub trait Object {
    fn id(&self) -> ObjectID;
    async fn dispatch(&self, request: Request) -> Result<Output>;
}

/// Handlers must implement this trait
#[async_trait]
pub trait Handler {
    async fn handle(&self, a: Tuple) -> Result<Output>;
}

pub struct Router {
    o: ObjectID,
    handlers: HashMap<String, Box<dyn Handler + Sync + Send>>,
}

impl Router {
    pub fn new(id: ObjectID) -> Router {
        Router {
            o: id,
            handlers: HashMap::new(),
        }
    }

    pub fn handle<S, H>(mut self, m: S, h: H) -> Router
    where
        S: Into<String>,
        H: Handler + Send + Sync + 'static,
    {
        self.handlers.insert(m.into(), Box::new(h));
        self
    }
}

#[async_trait]
impl Object for Router {
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
