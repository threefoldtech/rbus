use crate::request::{Arguments, ObjectID, Request, Response};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use thiserror::Error;

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

#[async_trait]
pub trait Handler {
    async fn handle(&self, a: Arguments) -> Result<Arguments>;
}

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
            id: request.id,
            arguments: args,
            error: err,
        }
    }
}
