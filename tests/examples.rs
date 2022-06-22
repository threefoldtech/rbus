#[macro_use]
extern crate anyhow;

use anyhow::{Context, Result};

use rbus::client;
use rbus::protocol;

use protocol::{ObjectID, Output, Request};
use rbus::server::{self, interface, Object};

#[interface]
pub trait Calculator {
    fn add(&self, a: f64, b: f64) -> Result<(f64, f64)>;
    fn divide(&self, a: f64, b: f64) -> Result<f64>;
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
        ObjectID::new("calculator", "1.0")
    }

    async fn dispatch(&self, request: Request) -> protocol::Result<Output> {
        match request.method.as_str() {
            "Add" => Ok(self
                .inner
                .add(request.inputs.at(0)?, request.inputs.at(1)?)
                .into()),
            "Divide" => Ok(self
                .inner
                .divide(request.inputs.at(0)?, request.inputs.at(1)?)
                .into()),
            _ => Err(protocol::Error::UnknownMethod(request.method)),
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
    fn add(&self, a: f64, b: f64) -> Result<(f64, f64)> {
        log::debug!("adding({}, {})", a, b);
        Ok((a + b, a - b))
    }
    fn divide(&self, a: f64, b: f64) -> Result<f64> {
        if b == 0.0 {
            bail!("cannot divide by zero")
        }
        Ok(a / b)
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

    async fn add(&self, a: f64, b: f64) -> protocol::Result<f64> {
        let req = Request::new(self.object.clone(), "Add").arg(a)?.arg(b)?;

        let mut client = self.client.clone();
        let out = client.request("server", req).await?;

        out.into()
    }

    async fn add_sub(&self, a: f64, b: f64) -> protocol::Result<(f64, f64)> {
        let req = Request::new(self.object.clone(), "AddSub").arg(a)?.arg(b)?;

        let mut client = self.client.clone();
        let out = client.request("server", req).await?;

        out.into()
    }

    async fn divide(&self, a: f64, b: f64) -> protocol::Result<f64> {
        let req = Request::new(self.object.clone(), "Divide").arg(a)?.arg(b)?;

        let mut client = self.client.clone();
        let out = client.request("server", req).await?;
        log::debug!("got output: {:?}", out);
        out.into()
    }
}

#[tokio::main]
async fn main_() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    // let client = client::Client::new("redis://localhost:6379").await?;

    // let calc = CalculatorStub::new(client);

    // println!("add(1,2) => {:?}", calc.add(1f64, 2f64).await);
    // println!("divide(1,2) => {:?}", calc.divide(1f64, 2f64).await);
    // println!("divide(1,0) => {:?}", calc.divide(1f64, 0f64).await);

    let pool = rbus::pool("redis://localhost:6379").await?;

    let calc = CalculatorObject::from(CalculatorImpl);

    let mut server = server::RedisServer::new(pool, "server", 3).await?;
    server.register(calc);

    println!("running server");
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

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let pool = rbus::pool("redis://localhost:6379").await?;

    let client = client::Client::new(pool);

    let calc = CalculatorStub::new(client);

    println!("making calls");
    println!("add(1,2) => {:?}", calc.add(1f64, 2f64).await);
    println!("add_sub(1,2) => {:?}", calc.add_sub(1f64, 2f64).await);
    println!("divide(1,2) => {:?}", calc.divide(10f64, 3f64).await);
    // println!("divide(1,0) => {:?}", calc.divide(1f64, 0f64).await);

    Ok(())
}
