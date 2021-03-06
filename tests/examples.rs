use anyhow::Result;
use serde::{Deserialize, Serialize};

use rbus::object;

// You can build your own complex object to pass around as
// inputs and outputs as long as they are serder serializable

#[derive(Serialize, Deserialize)]
pub struct Data {
    binary: Vec<u8>,
    str: String,
}

// annotate the service trait with `object` this will
// generate a usable server and client stubs.
// it accepts
// - name [optional] default to trait name
// - version [optional] default to 1.0
//
// NOTE:
// - only trait methods with first argument as receiver will be available for RPC
// - receiver must be a shared ref to self (&self)
// - all input arguments must be of type <T: Serialize>
// - return must be a Result (any Result) as long as the E type can be stringfied <E: Display>
// please check docs for `object` for more details
#[object(name = "calculator", version = "1.0")]
pub trait Calculator {
    // input and outputs can be anything according to the rules above
    fn add(&self, a: f64, b: f64) -> anyhow::Result<(f64, f64)>;
    #[rename("Divide")]
    fn divide(&self, a: f64, b: f64) -> Result<f64>;
    fn multiply(&self, a: f64, b: f64) -> Result<f64>;

    // methods can be declared async.
    async fn get_data(&self) -> Result<Data>;
}

// some implementation of our trait
struct CalculatorImpl;

/// async_trait is needed because we using async methods in tratis
#[async_trait::async_trait]
impl Calculator for CalculatorImpl {
    fn add(&self, a: f64, b: f64) -> Result<(f64, f64)> {
        log::debug!("adding({}, {})", a, b);
        Ok((a + b, a - b))
    }
    fn divide(&self, a: f64, b: f64) -> Result<f64> {
        if b == 0.0 {
            anyhow::bail!("cannot divide by zero")
        }
        Ok(a / b)
    }
    fn multiply(&self, a: f64, b: f64) -> Result<f64> {
        Ok(a * b)
    }
    async fn get_data(&self) -> Result<Data> {
        Ok(Data {
            binary: vec![],
            str: "Hello".into(),
        })
    }
}

#[tokio::test]
async fn full() {
    let pool = rbus::pool("redis://localhost:6379").await.unwrap();

    const MODULE: &str = "full-test";
    // build the object dispatcher
    let calc = CalculatorObject::from(CalculatorImpl);
    // create the module (server)
    let mut server = rbus::Server::new(pool.clone(), MODULE, 3).await.unwrap();
    // register the object
    server.register(calc);

    println!("running server");
    tokio::spawn(server.run());

    let client = rbus::Client::new(pool);

    // same as CalculatorObject, the CalculatorStub is auto generated in
    // this scope. Not the
    let calc = CalculatorStub::new(MODULE, client);

    assert_eq!((3f64, -1f64), calc.add(1f64, 2f64).await.unwrap());
    assert_eq!(5f64, calc.divide(10f64, 2f64).await.unwrap());

    use rbus::protocol::Error;

    assert!(matches!(
        calc.divide(10f64, 0f64).await,
        Err(Error::Call(err)) if err.message == "cannot divide by zero"));
}
