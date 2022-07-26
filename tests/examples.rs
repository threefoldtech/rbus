extern crate rbus;
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use protocol::ObjectID;
use rbus::protocol;
use rbus::server::{stream, Object, Receiver};

use bb8_redis::redis::{ErrorKind, RedisError};

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
// #[object(name = "calculator", version = "1.0")]
#[async_trait::async_trait]
pub trait Calculator {
    // input and outputs can be anything according to the rules above
    fn add(&self, a: f64, b: f64) -> anyhow::Result<(f64, f64)>;
    // #[rename("Divide")]
    fn divide(&self, a: f64, b: f64) -> Result<f64>;
    fn multiply(&self, a: f64, b: f64) -> Result<f64>;

    // methods can be declared async.
    async fn get_data(&self) -> Result<Data>;
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    data: String,
}
// some implementation of our trait
// struct CalculatorImpl;
// impl CalculatorImpl {
//     fn stream_test(&self) -> Receiver {
//         let (sender, receiver) = stream();
//         tokio::spawn(async move {
//             let mut count = 1;
//             loop {
//                 let msg = Message {
//                     data: format!("test {}", count),
//                 };
//                 match sender.send(msg).await {
//                     Ok(_) => {
//                         count += 1;
//                     }
//                     Err(_) => log::error!("failed to send data"),
//                 };
//                 sleep(Duration::from_secs(5));
//             }
//         });

//         return receiver;
//     }
// }

// /// async_trait is needed because we using async methods in tratis
// #[async_trait::async_trait]
// impl Calculator for CalculatorImpl {
//     fn add(&self, a: f64, b: f64) -> Result<(f64, f64)> {
//         log::debug!("adding({}, {})", a, b);
//         Ok((a + b, a - b))
//     }
//     fn divide(&self, a: f64, b: f64) -> Result<f64> {
//         if b == 0.0 {
//             anyhow::bail!("cannot divide by zero")
//         }
//         Ok(a / b)
//     }
//     fn multiply(&self, a: f64, b: f64) -> Result<f64> {
//         Ok(a * b)
//     }
//     async fn get_data(&self) -> Result<Data> {
//         Ok(Data {
//             binary: vec![],
//             str: "Hello".into(),
//         })
//     }
// }

struct Calc;
impl Calc {
    fn stream_test(&self) -> Receiver {
        let (sender, receiver) = stream();
        tokio::spawn(async move {
            let mut count = 1;
            loop {
                let data = format!("test {}", count);
                let msg = Message { data };
                println!("inside stream_test: {}", format!("test {}", count));
                match sender.send(msg).await {
                    Ok(_) => {
                        count += 1;
                    }
                    Err(_) => {
                        println!("failed to send data");
                        log::error!("failed to send data")
                    }
                };
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        return receiver;
    }
}
#[async_trait::async_trait]
impl Calculator for Calc {
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

#[async_trait::async_trait]
impl Object for Calc {
    fn id(&self) -> ObjectID {
        ObjectID::new("calculator", "1.0")
    }
    async fn dispatch(&self, request: protocol::Request) -> protocol::Result<protocol::Output> {
        match request.method.as_str() {
            "add" => {
                let x: f64 = request.inputs.at(0)?;
                let y: f64 = request.inputs.at(1)?;

                let output: Result<f64, &'static str> = Ok(x + y);
                Ok(output.into())
            }
            _ => Err(protocol::Error::UnknownMethod(request.method)),
        }
    }

    fn streams(&self) -> std::result::Result<HashMap<String, Receiver>, rbus::protocol::Error> {
        let mut streams = HashMap::new();
        let receiver = self.stream_test();
        streams.insert(String::from("stream_test"), receiver);
        return Ok(streams);
    }
}

#[tokio::test]
async fn full() {
    let pool = rbus::pool("redis://localhost:6379").await.unwrap();

    const MODULE: &str = "full-test";
    // build the object dispatcher
    // let calc = CalculatorObject::from(CalculatorImpl);
    let calc = Calc {};
    // create the module (server)
    let mut server = rbus::Server::new(pool.clone(), MODULE, 3).await.unwrap();
    // register the object
    server.register(calc);

    println!("running server");
    tokio::spawn(server.run());

    let client = rbus::Client::new(pool);
    let mut receiver = client.stream_test().await;
    // let join_handle = tokio::spawn(async move {
    loop {
        let msg = match receiver.recv().await {
            Some(msg) => msg,
            None => continue,
        };
        let message: Result<Message, RedisError> =
            rmp_serde::decode::from_read_ref(&msg).map_err(|err| {
                RedisError::from((
                    ErrorKind::TypeError,
                    "failed to decode request",
                    err.to_string(),
                ))
            });
        match message {
            Ok(message) => print!("got message {}", message.data),
            Err(_) => log::error!("failed to get message"),
        }
    }
    // });
    // join_handle.await.unwrap();
    // same as CalculatorObject, the CalculatorStub is auto generated in
    // this scope. Not the
    // let pool1 = rbus::pool("redis://localhost:6379").await.unwrap();
    // let client1 = rbus::Client::new(pool1);
    // let calc = CalculatorStub::new(MODULE, client1);

    // assert_eq!((3f64, -1f64), calc.add(1f64, 2f64).await.unwrap());
    // assert_eq!(5f64, calc.divide(10f64, 2f64).await.unwrap());

    // use rbus::protocol::Error;

    // assert!(matches!(
    //     calc.divide(10f64, 0f64).await,
    //     Err(Error::Call(err)) if err.message == "cannot divide by zero"));
}
