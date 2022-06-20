use crate::protocol::Container;
use crate::protocol::{Arguments, ObjectID, Request, Response};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use thiserror::Error;

pub mod redis;
pub use self::redis::Server as RedisServer;

/// Server errors
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("unknown method '{0}'")]
    UnknownMethod(String),
}

#[async_trait]
pub trait Object {
    fn id(&self) -> ObjectID;
    async fn dispatch(&self, request: Request) -> Response;
}

/// Handlers must implement this trait
#[async_trait]
pub trait Handler {
    async fn handle(&self, a: Arguments) -> Result<Arguments>;
}

/// Implements handler for functions
pub struct SyncHandler<F, T>
where
    F: Fn(Arguments) -> Result<T>,
{
    f: F,
}

impl<F, T> SyncHandler<F, T>
where
    F: Fn(Arguments) -> Result<T>,
{
    pub fn from(f: F) -> Self {
        SyncHandler { f }
    }
}

macro_rules! to_arguments {
    ($r:expr) => {{
        let mut args = $crate::protocol::Arguments::new();
        match $r {
            Ok(v) => {
                args.add(v)?;
                args.add(Option::<$crate::protocol::Error>::None)?;
            }
            Err(err) => {
                args.add(Option::<T>::None)?;
                args.add(Some($crate::protocol::Error::new(err.to_string())))?;
            }
        }
        args
    }};
}

#[async_trait]
impl<F, T> Handler for SyncHandler<F, T>
where
    F: Fn(Arguments) -> Result<T> + Send + Sync,
    T: serde::Serialize,
{
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        let result = (self.f)(a);
        Ok(to_arguments!(result))
    }
}

pub struct AsyncHandler<F, T, Fut>
where
    F: Fn(Arguments) -> Fut,
    Fut: Future<Output = Result<T>> + Send + Sync,
{
    f: F,
}

impl<F, T, Fut> AsyncHandler<F, T, Fut>
where
    F: Fn(Arguments) -> Fut,
    Fut: Future<Output = Result<T>> + Send + Sync,
{
    pub fn from(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<F, T, Fut> Handler for AsyncHandler<F, T, Fut>
where
    F: Fn(Arguments) -> Fut + Send + Sync,
    Fut: Future<Output = Result<T>> + Send + Sync,
    T: serde::Serialize,
{
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        let result = (self.f)(a).await;
        Ok(to_arguments!(result))
    }
}

pub struct AsyncHandlerWithState<F, T, Fut, S>
where
    F: Fn(S, Arguments) -> Fut,
    Fut: Future<Output = Result<T>> + Send + Sync,
    S: Clone + Send + Sync,
{
    s: S,
    f: F,
}

impl<F, T, Fut, S> AsyncHandlerWithState<F, T, Fut, S>
where
    F: Fn(S, Arguments) -> Fut,
    Fut: Future<Output = Result<T>> + Send + Sync,
    S: Clone + Send + Sync,
{
    pub fn from(s: S, f: F) -> Self {
        Self { s, f }
    }
}

#[async_trait]
impl<F, T, Fut, S> Handler for AsyncHandlerWithState<F, T, Fut, S>
where
    F: Fn(S, Arguments) -> Fut + Send + Sync,
    Fut: Future<Output = Result<T>> + Send + Sync,
    S: Clone + Send + Sync,
    T: serde::Serialize,
{
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        let result = (self.f)(self.s.clone(), a).await;
        Ok(to_arguments!(result))
    }
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

    async fn dispatch(&self, request: Request) -> Response {
        let handler = match self.handlers.get(&request.method) {
            Some(handler) => handler,
            None => {
                return Response {
                    id: request.id,
                    arguments: Arguments::new(),
                    error: Some(format!("{}", ServerError::UnknownMethod(request.method))),
                }
            }
        };

        let (args, err) = match handler.handle(request.arguments).await {
            Ok(args) => (args, None),
            Err(err) => (Arguments::new(), Some(format!("{}", err))),
        };

        Response {
            id: request.reply_to,
            arguments: args,
            error: err,
        }
    }
}
