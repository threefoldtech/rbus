use redis::{FromRedisValue, RedisResult, ToRedisArgs, Value};
use rmp_serde::Serializer;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::convert::TryFrom;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};

#[derive(Debug, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct CallError {
    pub message: String,
}

impl CallError {
    fn from<S: Into<String>>(message: S) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unknown object '{0}'")]
    UnknownObject(String),
    #[error("unknown method '{0}'")]
    UnknownMethod(String),
    #[error("no argument found at index {0}")]
    ArgumentOutOfRange(usize),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("encoding error: {0}")]
    Encoding(String),
    #[error("remote call failed with error '{0}'")]
    Call(CallError),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectID {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Version")]
    pub version: String,
}

impl ObjectID {
    pub fn new<S: Into<String>>(name: S, version: S) -> ObjectID {
        ObjectID {
            name: name.into(),
            version: version.into(),
        }
    }
}

impl Display for ObjectID {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        if self.version.is_empty() {
            write!(f, "{}", self.name)?;
        } else {
            write!(f, "{}@{}", self.name, self.version)?;
        }

        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Arguments(Vec<serde_bytes::ByteBuf>);

impl Arguments {
    pub fn at<'a, T>(&'a self, i: usize) -> Result<T>
    where
        T: Deserialize<'a>,
    {
        if i >= self.0.len() {
            return Err(Error::ArgumentOutOfRange(i));
        }

        rmp_serde::decode::from_read_ref(&self.0[i]).map_err(|e| Error::Encoding(e.to_string()))
    }

    pub fn add<T>(&mut self, o: T) -> Result<()>
    where
        T: Serialize,
    {
        let mut buffer: Vec<u8> = Vec::new();

        let encoder = Serializer::new(&mut buffer);
        let mut encoder = encoder.with_struct_map();
        o.serialize(&mut encoder)
            .map_err(|e| Error::Encoding(e.to_string()))?;

        self.0.push(ByteBuf::from(buffer));
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Debug for Arguments {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "Arguments(len: {})", self.0.len())
    }
}

impl<T, E> From<std::result::Result<T, E>> for Arguments
where
    T: Serialize,
    E: Display,
{
    fn from(res: std::result::Result<T, E>) -> Self {
        let mut args = Arguments::default();
        match res {
            Ok(t) => {
                args.add(t).unwrap();
                args.add(Option::<CallError>::None).unwrap();
            }
            Err(e) => {
                args.add(Option::<T>::None).unwrap();
                args.add(Some(CallError::from(e.to_string()))).unwrap();
            }
        };

        args
    }
}

impl<'a, A> From<Arguments> for Result<A>
where
    A: DeserializeOwned,
{
    fn from(args: Arguments) -> Self {
        let a: Option<A> = args.at(0)?;
        match a {
            Some(a) => Ok(a),
            None => {
                let e: CallError = args.at(1)?;
                Err(Error::Call(e))
            }
        }
    }
}

impl<A> TryFrom<Arguments> for (A,)
where
    A: DeserializeOwned,
{
    type Error = Error;
    fn try_from(args: Arguments) -> Result<Self> {
        Ok((args.at(0)?,))
    }
}

impl<A, B> TryFrom<Arguments> for (A, B)
where
    A: DeserializeOwned,
    B: DeserializeOwned,
{
    type Error = Error;
    fn try_from(args: Arguments) -> Result<Self> {
        Ok((args.at(0)?, args.at(1)?))
    }
}

impl<A, B, C> TryFrom<Arguments> for (A, B, C)
where
    A: DeserializeOwned,
    B: DeserializeOwned,
    C: DeserializeOwned,
{
    type Error = Error;
    fn try_from(args: Arguments) -> Result<Self> {
        Ok((args.at(0)?, args.at(1)?, args.at(2)?))
    }
}

impl<'a, A, B, C, D> TryFrom<Arguments> for (A, B, C, D)
where
    A: DeserializeOwned,
    B: DeserializeOwned,
    C: DeserializeOwned,
    D: DeserializeOwned,
{
    type Error = Error;
    fn try_from(args: Arguments) -> Result<Self> {
        Ok((args.at(0)?, args.at(1)?, args.at(2)?, args.at(3)?))
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Arguments")]
    pub arguments: Arguments,
    #[serde(rename = "Object")]
    pub object: ObjectID,
    #[serde(rename = "ReplyTo")]
    pub reply_to: String,
    #[serde(rename = "Method")]
    pub method: String,
}

impl Request {
    pub fn new<S: Into<String>>(object: ObjectID, method: S) -> Request {
        let id = uuid::Uuid::new_v4().to_string();
        // generate a new ID
        Request {
            object,
            id: id.clone(),
            method: method.into(),
            arguments: Arguments::default(),
            reply_to: id,
        }
    }

    pub fn arg<T>(mut self, argument: T) -> Result<Self>
    where
        T: Serialize,
    {
        self.arguments.add(argument)?;
        Ok(self)
    }
}

impl FromRedisValue for Request {
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let bytes = match v {
            Value::Data(bytes) => bytes,
            _ => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "expecting binary data",
                )))
            }
        };

        rmp_serde::decode::from_read_ref(bytes).map_err(|err| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "failed to decode request",
                err.to_string(),
            ))
        })
    }
}

impl ToRedisArgs for Request {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        let mut buffer: Vec<u8> = Vec::new();

        let encoder = Serializer::new(&mut buffer);
        let mut encoder = encoder.with_struct_map();
        self.serialize(&mut encoder)
            .expect("failed to encode response");

        out.write_arg(&buffer);
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Arguments")]
    pub arguments: Arguments,
    #[serde(rename = "Error")]
    pub error: Option<String>,
}

impl Response {
    pub fn is_error(&self) -> bool {
        matches!(&self.error, Some(e) if !e.is_empty())
    }
}

impl FromRedisValue for Response {
    fn from_redis_value(v: &Value) -> RedisResult<Self> {
        let bytes = match v {
            Value::Data(bytes) => bytes,
            _ => {
                return Err(redis::RedisError::from((
                    redis::ErrorKind::TypeError,
                    "expecting binary data",
                )))
            }
        };

        rmp_serde::decode::from_read_ref(bytes).map_err(|err| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "failed to decode request",
                err.to_string(),
            ))
        })
    }
}

impl ToRedisArgs for Response {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + redis::RedisWrite,
    {
        let mut buffer: Vec<u8> = Vec::new();

        let encoder = Serializer::new(&mut buffer);
        let mut encoder = encoder.with_struct_map();
        self.serialize(&mut encoder)
            .expect("failed to encode response");

        out.write_arg(&buffer);
    }
}
