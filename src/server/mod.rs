use crate::request::{Arguments, ObjectID, Request, Response};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use thiserror::Error;
pub mod redis;

/// Server errors
#[derive(Error, Debug)]
pub enum ServerError {
    #[error("unknown method '{0}'")]
    UnknownMethod(String),
}

#[async_trait]
pub trait Service {
    fn id(&self) -> ObjectID;
    async fn dispatch(&self, request: Request) -> Response;
}

/// Handlers must implement this trait
#[async_trait]
pub trait Handler {
    async fn handle(&self, a: Arguments) -> Result<Arguments>;
}

/// Implements handler for functions
pub struct SyncHandler<F>
where
    F: Fn(Arguments) -> Result<Arguments>,
{
    f: F,
}

impl<F> SyncHandler<F>
where
    F: Fn(Arguments) -> Result<Arguments>,
{
    pub fn from(f: F) -> Self {
        SyncHandler { f }
    }
}

#[async_trait]
impl<F> Handler for SyncHandler<F>
where
    F: Fn(Arguments) -> Result<Arguments> + Send + Sync,
{
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        (self.f)(a)
    }
}

pub struct AsyncHandler<F, Fut>
where
    F: Fn(Arguments) -> Fut,
    Fut: Future<Output = Result<Arguments>> + Send + Sync,
{
    f: F,
}

impl<F, Fut> AsyncHandler<F, Fut>
where
    F: Fn(Arguments) -> Fut,
    Fut: Future<Output = Result<Arguments>> + Send + Sync,
{
    pub fn from(f: F) -> Self {
        Self { f }
    }
}

#[async_trait]
impl<F, Fut> Handler for AsyncHandler<F, Fut>
where
    F: Fn(Arguments) -> Fut + Send + Sync,
    Fut: Future<Output = Result<Arguments>> + Send + Sync,
{
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        (self.f)(a).await
    }
}

pub struct AsyncHandlerWithState<F, Fut, S>
where
    F: Fn(S, Arguments) -> Fut,
    Fut: Future<Output = Result<Arguments>> + Send + Sync,
    S: Clone + Send + Sync,
{
    s: S,
    f: F,
}

impl<F, Fut, S> AsyncHandlerWithState<F, Fut, S>
where
    F: Fn(S, Arguments) -> Fut,
    Fut: Future<Output = Result<Arguments>> + Send + Sync,
    S: Clone + Send + Sync,
{
    pub fn from(f: F, s: S) -> Self {
        Self { f, s }
    }
}

#[async_trait]
impl<F, Fut, S> Handler for AsyncHandlerWithState<F, Fut, S>
where
    F: Fn(S, Arguments) -> Fut + Send + Sync,
    Fut: Future<Output = Result<Arguments>> + Send + Sync,
    S: Clone + Send + Sync,
{
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        (self.f)(self.s.clone(), a).await
    }
}

#[macro_export]
macro_rules! sync_handler {
    ($i:ident) => {{
        use $crate::server::SyncHandler;
        SyncHandler::from($i)
    }};
}

#[macro_export]
macro_rules! async_handler {
    ($i:ident) => {{
        use $crate::server::AsyncHandler;
        AsyncHandler::from($i)
    }};
}

pub use async_handler;
pub use sync_handler;

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
impl Service for Router {
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
