pub mod local;
pub mod s3;

use tokio::fs::File;
use crate::error::Result;
use rocket::futures::StreamExt;
use rocket::Request;
use rocket::data::DataStream;
use rocket::response::Responder;
use rocket::response::stream::ByteStream;

pub enum NarResponder {
    File(File),
    Stream(hyper::Body),
}

impl<'r> Responder<'r, 'r> for NarResponder {
    fn respond_to(self, req: &'r Request<'_>) -> rocket::response::Result<'r> {
        let response = match self {
            NarResponder::File(file) => file.respond_to(req)?,
            NarResponder::Stream(stream) => {
                let foo = ByteStream::from(stream.map(|x| x.unwrap()));
                foo.respond_to(req)?
            },
        };
        Ok(response)
    }
}

#[async_trait::async_trait]
pub trait Backend {
    async fn read_nar(&self, url: &str) -> Result<NarResponder>;
    async fn write_nar(&self, url: &str, reader: &mut DataStream<'_>) -> Result<()>;
    async fn finish_nar(&self, url: &str) -> Result<()>;
}
