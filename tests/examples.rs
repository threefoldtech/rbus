#[macro_use]
extern crate anyhow;

use anyhow::Result;

use rbus::client;
use rbus::protocol;

use protocol::{ObjectID, Request};
use rbus::server::{self, interface};

#[interface(name = "calculator", version = "0.2")]
pub trait Calculator {
    fn add(&self, a: f64, b: f64) -> anyhow::Result<(f64, f64)>;
    fn divide(&self, a: f64, b: f64) -> Result<f64>;
    fn multiply(&self, a: f64, b: f64) -> Result<f64>;
    async fn something(&self) -> Result<()>;
}

struct CalculatorImpl;

#[async_trait::async_trait]
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
    fn multiply(&self, a: f64, b: f64) -> Result<f64> {
        Ok(a * b)
    }
    async fn something(&self) -> Result<()> {
        Ok(())
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

    let calc = CalculatorStub::new("server", client);

    println!("making calls");
    println!("add(1,2) => {:?}", calc.add(1f64, 2f64).await);
    println!("divide(10,3) => {:?}", calc.divide(10f64, 3f64).await);
    println!("divide(10,0) => {:?}", calc.divide(10f64, 0f64).await);
    // println!("divide(1,0) => {:?}", calc.divide(1f64, 0f64).await);

    Ok(())
}
