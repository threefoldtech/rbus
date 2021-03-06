# ZBUS
[`zbus`](https://github.com/threefoldtech/zbus) implementation in rust (hence the name `rbus`). It ss a framework for `local` RPC calls. User of this protocol will enable separate processes to make rpc calls to each other without the knowledge of how to reach the other component directly or where it actually live on the system.

This is accomplished by using redis as a message bus. But calls still need to know:
- the module name
- object identity
- method names that are allowed to be called over the bus
- exact arguments number and types
- exact return type(s)

```
module(name) {
    object(name, version) {
        method[name](arguments ...) -> output
    }
    object(name, version) {
        ...
    }
}
```

Hence all implementers of this protocol usually provide tool set to generate both server and client stubs.

While `rbus` provide you the tools to manually build server and client side stubs. It also provide you with a convenient macro to generate both server and client stubs

```rust
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
#[object(name = "calculator", version = "0.2")]
pub trait Calculator {
    // input and outputs can be anything according to the rules above
    fn add(&self, a: f64, b: f64) -> anyhow::Result<(f64, f64)>;

    // to be able to do calls across languages, you can rename
    // the method (over wire) to be able to call methods in Golang
    // for example
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

#[tokio::main]
async fn main() -> Result<()> {
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    let pool = rbus::pool("redis://localhost:6379").await?;

    let server = true;
    if server {
        // build the object dispatcher
        let calc = CalculatorObject::from(CalculatorImpl);
        // create the module (server)
        let mut server = rbus::Server::new(pool, "server", 3).await?;
        // register the object
        server.register(calc);

        println!("running server");
        server.run().await;
    } else {
        let client = rbus::Client::new(pool);

        // same as CalculatorObject, the CalculatorStub is auto generated in
        // this scope. Not the
        let calc = CalculatorStub::new("server", client);

        println!("making calls");
        println!("add(1,2) => {:?}", calc.add(1f64, 2f64).await);
        println!("divide(10,3) => {:?}", calc.divide(10f64, 3f64).await);
        println!("divide(10,0) => {:?}", calc.divide(10f64, 0f64).await);
    }

    Ok(())
}
```
