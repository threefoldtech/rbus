use crate::request::{Arguments, ObjectID, Request, Response};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
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
trait Handler {
    async fn handle(&self, a: Arguments) -> Result<Arguments>;
}

//type Handler = fn(Arguments) -> Result<Arguments>;

pub struct Router {
    o: ObjectID,
    handlers: HashMap<String, Box<dyn Handler>>,
}

impl Router {
    pub fn new(id: ObjectID) -> Router {
        Router {
            o: id,
            handlers: HashMap::new(),
        }
    }

    pub fn handle<S, H>(&mut self, m: S, h: Box<dyn Handler + Send>) -> &mut Router
    where
        S: Into<String>,
    {
        self.handlers.insert(m.into(), h);
        &mut self
    }
}

#[async_trait]
impl Service for Router {
    fn id(&self) -> ObjectID {
        self.o.clone()
    }

    async fn dispatch(&self, request: Request) -> Response {
        unimplemented!();

        // let handler = match self.handlers.get(&request.method) {
        //     Some(handler) => handler,
        //     None => {
        //         return Response {
        //             id: request.id,
        //             arguments: Arguments::new(),
        //             error: Some(format!("{}", ServerError::UnknownMethod(request.method))),
        //         }
        //     }
        // };

        // let (args, err) = match handler.handle(request.arguments).await {
        //     Ok(args) => (args, None),
        //     Err(err) => (Arguments::new(), Some(format!("{}", err))),
        // };

        // Response {
        //     id: request.id,
        //     arguments: args,
        //     error: err,
        // }
    }
}
