extern crate rbus;
use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use protocol::ObjectID;
use rbus::client::Receiver;
use rbus::server::{Object, Sender, Sink};
use rbus::{object, protocol};
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
#[async_trait::async_trait]
pub trait Calculator {
    // input and outputs can be anything according to the rules above
    fn add(&self, a: f64, b: f64) -> anyhow::Result<(f64, f64)>;

    #[rename("Divide")]
    fn divide(&self, a: f64, b: f64) -> Result<f64>;
    fn multiply(&self, a: f64, b: f64) -> Result<f64>;

    // methods can be declared async.
    async fn get_data(&self) -> Result<Data>;

    #[stream]
    async fn date(&self, rec: Sender<u32>);

    #[stream]
    async fn names(&self, rec: Sender<String>);
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    data: String,
}

// some implementation of our trait
#[derive(Clone)]
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

    async fn date(&self, rec: Sender<u32>) {
        loop {
            // sleep
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = rec.send(&10).await;
        }
    }

    async fn names(&self, rec: Sender<String>) {
        let name = "Ashraf".to_owned();
        loop {
            // sleep
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let _ = rec.send(&name).await;
        }
    }
}

struct StreamTest;
impl StreamTest {
    fn stream_test(&self) -> Sink {
        log::debug!("stream TEST started");
        let (sender, receiver) = Sender::new();
        tokio::spawn(async move {
            let mut count = 1;
            loop {
                let data = format!("test {}", count);
                let msg = Message { data };
                log::debug!("sending event");
                match sender.send(&msg).await {
                    Ok(_) => {
                        count += 1;
                    }
                    Err(_) => {
                        log::error!("failed to send data")
                    }
                };
                log::debug!("event sent");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        return receiver;
    }
}

#[async_trait::async_trait]
impl Object for StreamTest {
    fn id(&self) -> ObjectID {
        ObjectID::new("streamer", "1.0")
    }

    async fn dispatch(&self, request: protocol::Request) -> protocol::Result<protocol::Output> {
        Err(protocol::Error::UnknownMethod(request.method))
    }

    fn streams(&self) -> std::result::Result<HashMap<String, Sink>, rbus::protocol::Error> {
        let mut streams = HashMap::new();
        let receiver = self.stream_test();
        streams.insert("test".into(), receiver);
        return Ok(streams);
    }
}

#[tokio::test]
async fn test_encode() {
    let msg = Message {
        data: "test".to_string(),
    };
    let encoded = protocol::encode(&msg).unwrap();
    let message: Message = rmp_serde::decode::from_read_ref(&encoded).unwrap();
    assert_eq!(msg.data, message.data);
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

    let client = rbus::Client::new("redis://localhost:6379").await.unwrap();

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

#[tokio::test]
async fn testing_streams() {
    simple_logger::init_with_level(log::Level::Debug).unwrap();

    let pool = rbus::pool("redis://localhost:6379").await.unwrap();

    const MODULE: &str = "full-test";
    // build the object dispatcher
    // let calc = CalculatorObject::from(CalculatorImpl);
    let calc = StreamTest;
    // create the module (server)
    let mut server = rbus::Server::new(pool.clone(), MODULE, 3).await.unwrap();
    // register the object
    server.register(calc);

    tokio::spawn(server.run());

    //tokio::time::sleep(Duration::from_secs(3)).await;
    let client = rbus::Client::new("redis://localhost:6379").await.unwrap();
    let mut receiver: Receiver<Message> = client
        .stream(MODULE, ObjectID::new("streamer", "1.0"), "test")
        .await
        .unwrap();
    // let join_handle = tokio::spawn(async move {
    loop {
        let msg = match receiver.recv().await {
            Some(msg) => match msg {
                Ok(msg) => msg,
                Err(err) => {
                    panic!("error decoding message: {}", err)
                }
            },
            None => {
                log::debug!("stream terminated!");
                break;
            }
        };

        log::debug!("got a message {:?}", msg);
        // terminate test.
        if msg.data == "test 3" {
            // why this hangs forever. instead of return
            // the problem is when we "return" and the receiver is dropped, the subscribe thread is
            // not terminated because it still waiting (subscribed) to the redis channel. Hence
            // the thread never get a chance to "receive" the event (because server was stopped on return)
            // hence the thread doesn't get a chance to try to send over the channel so it never
            // detects that the receiver was dropped. Hence the thread is blocked forever.
            // This however is because both the server and the client are running in the same
            // process which is not a valid case. If the server is running separately it will keep
            // sending events hence eventually the client subscribe will terminate

            // the problem is this scenario can happen in real life if server exited but clients are still
            // waiting. hence a good solution is needed for client cancellation
            break;
        }
    }
}
