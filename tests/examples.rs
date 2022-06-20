#[macro_use]
extern crate anyhow;

use anyhow::{Context, Result};

use rbus::client;
use rbus::protocol;
use rbus::server;

use protocol::{ObjectID, Request, Values};

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

    async fn add(&self, a: f64, b: f64) -> Result<f64> {
        let req = Request::new(self.object.clone(), "Add")
            .arg(a)
            .context("failed to encode `a`")?
            .arg(b)
            .context("failed to encode `b`")?;

        let mut client = self.client.clone();
        let (x,): (f64,) = client.request("server", req).await?.values()?;

        Ok(x)
    }

    async fn divide(&self, a: f64, b: f64) -> Result<f64> {
        let req = Request::new(self.object.clone(), "Divide")
            .arg(a)
            .context("failed to add first argument")?
            .arg(b)
            .context("failed to add second argument")?;

        let mut client = self.client.clone();
        let response = client.request("server", req).await?;

        let (v, e): (f64, Option<protocol::Error>) = response.values()?;
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

    let router = server::Router::new(ObjectID::new("tester", "1.0"))
        .handle("hello", server::SyncHandler::from(hello))
        .handle("add", server::AsyncHandler::from(add))
        .handle("state", server::AsyncHandlerWithState::from(10, pingState));

    let mut server = server::RedisServer::new(pool, "server", 3).await?;
    server.register(router);

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

fn hello(input: protocol::Arguments) -> Result<protocol::Arguments> {
    let name = protocol::inputs!(input, String)?;
    Ok(protocol::returns!(format!("hello {}", name)))
}

async fn add(input: protocol::Arguments) -> Result<protocol::Arguments> {
    let (a, b) = protocol::inputs!(input, f64, f64)?;
    println!("adding {} + {}", a, b);
    Ok(protocol::returns!(a + b))
}

async fn pingState(this: i64, input: protocol::Arguments) -> Result<String> {
    let name = protocol::inputs!(input, String)?;
    Ok(format!("pong {} {}", name, this))
}
