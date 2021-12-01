use anyhow::{Context, Result};
use rmp_serde::Serializer;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::fmt::{Display, Formatter, Result as FmtResult};

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
        if self.version.len() == 0 {
            write!(f, "{}", self.name)?;
        } else {
            write!(f, "{}@{}", self.name, self.version)?;
        }

        Ok(())
    }
}

pub trait Container {
    fn decode<'a, T>(&'a self, i: usize) -> Result<T>
    where
        T: Deserialize<'a>;
    fn add<T>(&mut self, o: T) -> Result<()>
    where
        T: Serialize;
}

pub type Arguments = Vec<serde_bytes::ByteBuf>;

impl Container for Arguments {
    fn decode<'a, T>(&'a self, i: usize) -> Result<T>
    where
        T: Deserialize<'a>,
    {
        Ok(rmp_serde::decode::from_read_ref(&self[i])?)
    }
    fn add<T>(&mut self, o: T) -> Result<()>
    where
        T: Serialize,
    {
        let mut buffer: Vec<u8> = Vec::new();

        let encoder = Serializer::new(&mut buffer);
        let mut encoder = encoder.with_struct_map();
        o.serialize(&mut encoder)
            .context("failed to encode argument")?;

        self.push(ByteBuf::from(buffer));
        Ok(())
    }
}

#[macro_export]
macro_rules! returns {
    ($($i:expr),*) => {{
        use $crate::request::{Arguments, Container};
        let mut args = Arguments::new();

        $(args.add($i).unwrap();)*
        args
    }};
}

#[macro_export]
macro_rules! inputs {
    ($i:expr, $t:ty) => {
        {
            use $crate::request::{Values};
            use anyhow::Result;
            let t: Result<($t,)> = $i.values();
            t.map(|v| v.0)
        }
    };
    ($i:expr, $($t:ty),+) => {{
        use $crate::request::{Values};
        use anyhow::Result;
        let t: Result<($($t),+)> = $i.values();
        t
    }};

}

pub use inputs;
pub use returns;

pub trait Values<'a, T> {
    fn values(&'a self) -> Result<T>;
}

impl<'a, A> Values<'a, (A,)> for Arguments
where
    A: Deserialize<'a>,
{
    fn values(&'a self) -> Result<(A,)> {
        let a: A = self.decode(0)?;
        Ok((a,))
    }
}

impl<'a, A, B> Values<'a, (A, B)> for Arguments
where
    A: Deserialize<'a>,
    B: Deserialize<'a>,
{
    fn values(&'a self) -> Result<(A, B)> {
        let a: A = self.decode(0)?;
        let b: B = self.decode(1)?;
        Ok((a, b))
    }
}

impl<'a, A, B, C> Values<'a, (A, B, C)> for Arguments
where
    A: Deserialize<'a>,
    B: Deserialize<'a>,
    C: Deserialize<'a>,
{
    fn values(&'a self) -> Result<(A, B, C)> {
        let a: A = self.decode(0)?;
        let b: B = self.decode(1)?;
        let c: C = self.decode(2)?;
        Ok((a, b, c))
    }
}

impl<'a, A, B, C, D> Values<'a, (A, B, C, D)> for Arguments
where
    A: Deserialize<'a>,
    B: Deserialize<'a>,
    C: Deserialize<'a>,
    D: Deserialize<'a>,
{
    fn values(&'a self) -> Result<(A, B, C, D)> {
        let a: A = self.decode(0)?;
        let b: B = self.decode(1)?;
        let c: C = self.decode(2)?;
        let d: D = self.decode(3)?;
        Ok((a, b, c, d))
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
            id: id.clone(),
            object: object,
            method: method.into(),
            arguments: vec![],
            reply_to: id,
        }
    }

    pub fn from_slice(slice: &[u8]) -> Result<Request> {
        Ok(rmp_serde::decode::from_read_ref(slice)?)
    }

    pub fn add_argument<T>(mut self, argument: T) -> Result<Self>
    where
        T: Serialize,
    {
        self.arguments.add(argument)?;
        Ok(self)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buffer: Vec<u8> = Vec::new();

        let encoder = Serializer::new(&mut buffer);
        let mut encoder = encoder.with_struct_map();
        self.serialize(&mut encoder)
            .context("failed to encode request")?;

        Ok(buffer)
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
    pub fn from_slice(slice: &[u8]) -> Result<Response> {
        Ok(rmp_serde::decode::from_read_ref(slice)?)
    }

    pub fn is_error(&self) -> bool {
        matches!(&self.error, Some(e) if e.len() > 0)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut buffer: Vec<u8> = Vec::new();

        let encoder = Serializer::new(&mut buffer);
        let mut encoder = encoder.with_struct_map();
        self.serialize(&mut encoder)
            .context("failed to encode response")?;
        Ok(buffer)
    }
}

#[derive(Serialize, Deserialize, Debug, thiserror::Error)]
#[error("{}", message)]
pub struct Error {
    #[serde(rename = "Message")]
    pub message: String,
}

#[cfg(test)]
mod test {
    #[test]
    fn returns() {
        //let x = 1;
        let args = returns!(1, 2, 3);
        assert_eq!(args.len(), 3);
    }
    #[test]
    fn inputs() {
        let args = returns!(1, 2, 3);
        assert_eq!(args.len(), 3);

        let (a, b, c) = inputs!(args, i64, i64, i64).unwrap();
        assert_eq!(a, 1);
        assert_eq!(b, 2);
        assert_eq!(c, 3);
    }
}
