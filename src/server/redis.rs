use super::Service;
use crate::request;
use anyhow::{Context, Result};
use redis::{aio::ConnectionManager, Client as Redis};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};

const PULL_TIMEOUT: i32 = 10;
const RESPONSE_TTL: i32 = 5 * 60;

type Routers = HashMap<String, Box<dyn Service + Send + Sync>>;

pub struct Server {
    module: String,
    client: Redis,
    workers: u32,
    routers: Routers,
}

type MessageSender = oneshot::Sender<request::Request>;
type ChannelSender = mpsc::Sender<MessageSender>;

impl Server {
    pub async fn new<S>(module: S, url: S, workers: u32) -> Result<Server>
    where
        S: AsRef<str>,
    {
        assert!(workers > 1, "workers must be at least 1");

        let client = Redis::open(url.as_ref())?;

        Ok(Server {
            client,
            workers,
            module: module.as_ref().into(),
            routers: Routers::new(),
        })
    }

    pub fn register<T>(&mut self, service: T)
    where
        T: Service + Send + Sync + 'static,
    {
        self.routers
            .insert(service.id().to_string(), Box::new(service));
    }

    pub async fn run(self) -> Result<()> {
        // routers can not be changed afterwords. so we need to spawn workers here
        // and pass them a copy of the routers, and a way for them to pull for messages.

        let routers = self.routers;
        let mut cmd = redis::cmd("BLPOP");

        //let mut args: Vec<String> = vec![];
        for (key, _) in routers.iter() {
            cmd.arg(format!("{}.{}", self.module, key));
        }

        cmd.arg(format!("{}", PULL_TIMEOUT));

        let (tx, mut rx) = mpsc::channel::<MessageSender>(1);

        let routers = Arc::new(routers);

        for _ in 0..self.workers - 1 {
            let worker = Worker::new(self.client.clone(), Arc::clone(&routers));
            tokio::spawn(worker.work(tx.clone()));
        }

        let mut con = self.client.clone().get_tokio_connection_manager().await?;
        while let Some(sender) = rx.recv().await {
            // fetch message from queue, then push to sender
            loop {
                // we have this done in a loop so we make sure we can
                // renew the connection if it failed
                let result: redis::RedisResult<Option<(String, Vec<u8>)>> =
                    cmd.query_async(&mut con).await;
                let (queue, payload) = match result {
                    Ok(Some((queue, payload))) => (queue, payload),
                    Ok(None) => continue,
                    Err(err) => {
                        // sleep for few seconds and try again
                        log::error!("failed to get next message: {}", err);
                        sleep(Duration::from_secs(3)).await;
                        continue;
                    }
                };

                log::debug!("received call: {}", queue);
                let request = match request::Request::from_slice(&payload) {
                    Ok(request) => request,
                    Err(err) => {
                        log::error!("failed to decode message from queue '{}': {}", queue, err);
                        break;
                    }
                };

                log::debug!("method: {}.{}", queue, request.method);
                if let Err(_request) = sender.send(request) {
                    // todo: to avoid request loss, may be this should be pushed
                    // back to the same queue!
                    log::error!("failed to push message to work");
                }
                break;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Worker {
    routers: Arc<Routers>,
    client: Redis,
}

impl Worker {
    fn new(client: Redis, routers: Arc<Routers>) -> Self {
        Self { client, routers }
    }

    async fn work(self, tx: ChannelSender) {
        'next: loop {
            let (ms, mr) = oneshot::channel::<request::Request>();
            // if sent failed, means receiver has shutdown, so it's safe to return
            if let Err(_) = tx.send(ms).await {
                return;
            }

            let message = match mr.await {
                Ok(message) => message,
                Err(err) => {
                    log::error!("failed to receive message from server: {}", err);
                    continue;
                }
            };

            // dispatch message to handlers.
            let response = match self.routers.get(&message.object.to_string()) {
                Some(service) => service.dispatch(message).await,
                None => request::Response {
                    id: message.id,
                    arguments: request::Arguments::new(),
                    error: Some("unknown module".into()),
                },
            };

            // encode response
            let data = match response.encode() {
                Ok(data) => data,
                Err(err) => {
                    log::error!("failed to encode response: {}", err);
                    continue;
                }
            };

            loop {
                let mut con = match self.client.get_async_connection().await {
                    Ok(con) => con,
                    Err(err) => {
                        log::error!("failed to get redis connection: {}", err);
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                };

                let result: redis::RedisResult<()> = redis::cmd("RPUSH")
                    .arg(&response.id)
                    .arg(&data)
                    .query_async(&mut con)
                    .await;
                if let Err(err) = result {
                    log::error!("failed to push result to redis: {}", err);
                    continue 'next;
                }
                let result: redis::RedisResult<()> = redis::cmd("EXPIRE")
                    .arg(&response.id)
                    .arg(RESPONSE_TTL)
                    .query_async(&mut con)
                    .await;
                if let Err(err) = result {
                    log::error!("failed to push result to redis: {}", err);
                    continue 'next;
                }

                break;
            }
        }
    }
}
