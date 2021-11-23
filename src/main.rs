#[macro_use]
extern crate anyhow;

use anyhow::{Context, Result};

pub mod client;
#[macro_use]
pub mod request;
pub mod server;
use request::{ObjectID, Request, Values};
use server::Service;

struct CalculatorStub {
    client: client::Client,
    object: request::ObjectID,
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
            .add_argument(a)
            .context("failed to encode `a`")?
            .add_argument(b)
            .context("failed to encode `b`")?;

        let mut client = self.client.clone();
        let (x,): (f64,) = client.request("server", req).await?.values()?;

        Ok(x)
    }

    async fn divide(&self, a: f64, b: f64) -> Result<f64> {
        let req = Request::new(self.object.clone(), "Divide")
            .add_argument(a)
            .context("failed to add first argument")?
            .add_argument(b)
            .context("failed to add second argument")?;

        let mut client = self.client.clone();
        let response = client.request("server", req).await?;

        let (v, e): (f64, Option<client::Error>) = response.values()?;
        if let Some(err) = e {
            bail!(err);
        }

        Ok(v)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // let client = client::Client::new("redis://localhost:6379").await?;

    // let calc = CalculatorStub::new(client);

    // println!("add(1,2) => {:?}", calc.add(1f64, 2f64).await);
    // println!("divide(1,2) => {:?}", calc.divide(1f64, 2f64).await);
    // println!("divide(1,0) => {:?}", calc.divide(1f64, 0f64).await);

    let router = server::Router::new(ObjectID::new("tester", "1.0"))
        .handle("hello", server::handler!(hello))
        .handle("ping", server::async_handler!(ping));

    let req = Request::new(router.id(), "ping");
    let req = req.add_argument("azmy")?;
    let response = router.dispatch(req).await;

    println!("response: {:?}", response);
    let answer = request::inputs!(response.arguments, String).unwrap();
    println!("answer: {}", answer);
    Ok(())
}

fn hello(input: request::Arguments) -> Result<request::Arguments> {
    let name = request::inputs!(input, String)?;
    Ok(request::returns!(format!("hello {}", name)))
}

async fn ping(input: request::Arguments) -> Result<request::Arguments> {
    let name = request::inputs!(input, String)?;
    Ok(request::returns!(format!("pong {}", name)))
}

struct M {}
impl M {
    pub async fn test(input: request::Arguments) -> Result<request::Arguments> {
        bail!("failed")
    }
}
