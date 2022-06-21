#[macro_use]
extern crate anyhow;

use std::convert::TryInto;

use anyhow::{Context, Result};

use rbus::client;
use rbus::protocol;

use protocol::{Arguments, ObjectID, Request, Values};
use rbus::server::{self, Object};

pub trait Calculator {
    fn add(&self, a: f64, b: f64) -> Result<f64>;
}

pub struct CalculatorObject<T>
where
    T: Calculator,
{
    inner: T,
}

#[async_trait::async_trait]
impl<T> Object for CalculatorObject<T>
where
    T: Calculator + Send + Sync + 'static,
{
    fn id(&self) -> ObjectID {
        ObjectID::new("Calculator", "1.0")
    }

    async fn dispatch(&self, request: Request) -> protocol::Result<Arguments> {
        match request.method.as_str() {
            "add" => {
                let a = request.arguments.at(0)?;
                let b = request.arguments.at(1)?;

                Ok(self.inner.add(a, b).into())
            }
            _ => Err(protocol::RemoteError::UnknownMethod(request.method)),
        }
    }
}

impl<T> From<T> for CalculatorObject<T>
where
    T: Calculator,
{
    fn from(inner: T) -> Self {
        Self { inner }
    }
}

struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    fn add(&self, a: f64, b: f64) -> Result<f64> {
        Ok(a + b)
    }
}

struct CalculatorStub {
    client: client::Client,
    object: protocol::ObjectID,
}

impl CalculatorStub {
    fn new(client: client::Client) -> CalculatorStub {
        CalculatorStub {
            client,
            object: ObjectID::new("calculator", "1.0"),
        }
    }

    async fn add(&self, a: f64, b: f64) -> protocol::Result<(f64, f64)> {
        let req = Request::new(self.object.clone(), "Add").arg(a)?.arg(b)?;

        let mut client = self.client.clone();
        let out = client.request("server", req).await.unwrap();

        out.into()

        // let (a,): (f64,) = out.try_into()?;

        // Ok(a)
    }

    async fn divide(&self, a: f64, b: f64) -> Result<f64> {
        let req = Request::new(self.object.clone(), "Divide")
            .arg(a)
            .context("failed to add first argument")?
            .arg(b)
            .context("failed to add second argument")?;

        let mut client = self.client.clone();
        let response = client.request("server", req).await?;

        let (v, e): (f64, Option<protocol::RemoteError>) = response.values()?;
        if let Some(err) = e {
            bail!(err);
        }

        Ok(v)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    // let client = client::Client::new("redis://localhost:6379").await?;

    // let calc = CalculatorStub::new(client);

    // println!("add(1,2) => {:?}", calc.add(1f64, 2f64).await);
    // println!("divide(1,2) => {:?}", calc.divide(1f64, 2f64).await);
    // println!("divide(1,0) => {:?}", calc.divide(1f64, 0f64).await);

    let pool = rbus::pool("redis:://localhost:6379").await?;

    let calc = CalculatorObject::from(CalculatorImpl);

    let mut server = server::RedisServer::new(pool, "server", 3).await?;
    server.register(calc);

    server.run().await;
    Ok(())

    // let req = Request::new(router.id(), "state");
    // let req = req.add_argument("azmy")?;
    // let response = router.dispatch(req).await;

    // println!("response: {:?}", response);
    // let answer = request::inputs!(response.arguments, String).unwrap();
    // println!("answer: {}", answer);
    // Ok(())
}
