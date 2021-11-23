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

pub struct HandlerFn(fn(Arguments) -> Result<Arguments>);

impl HandlerFn {
    pub fn from(f: fn(Arguments) -> Result<Arguments>) -> HandlerFn {
        HandlerFn(f)
    }
}

#[async_trait]
impl Handler for HandlerFn {
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        self.0(a)
    }
}

pub type AsyncFn = fn(Arguments) -> Pin<Box<dyn Future<Output = Result<Arguments>> + Send>>;

pub struct HandlerAsyncFn(AsyncFn);

impl HandlerAsyncFn {
    pub fn from(f: AsyncFn) -> HandlerAsyncFn {
        HandlerAsyncFn(f)
    }
}

#[macro_export]
macro_rules! handler {
    ($i:ident) => {{
        use $crate::server::HandlerFn;
        HandlerFn::from($i)
    }};
}

#[macro_export]
macro_rules! async_handler {
    ($i:ident) => {{
        use anyhow::Result;
        use std::future::Future;
        use std::pin::Pin;
        use $crate::request::Arguments;
        use $crate::server::HandlerAsyncFn;

        HandlerAsyncFn::from(
            |input: Arguments| -> Pin<Box<dyn Future<Output = Result<Arguments>> + Send>> {
                Box::pin($i(input))
            },
        )
    }};
}

pub use async_handler;
pub use handler;

#[async_trait]
impl Handler for HandlerAsyncFn {
    async fn handle(&self, a: Arguments) -> Result<Arguments> {
        self.0(a).await
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
