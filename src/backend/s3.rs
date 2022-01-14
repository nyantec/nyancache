use super::{Backend, NarResponder};
use s3::bucket::Bucket;
use s3::command::{Command, HttpMethod};
use s3::request::Reqwest;
use s3::request_trait::Request;
use rocket::data::DataStream;
use crate::error::{Error, Result};
use cached::proc_macro::cached;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper::{Method, Client, Body, client::HttpConnector};
use std::path::PathBuf;

#[cached]
pub fn https_client() -> Client<HttpsConnector<HttpConnector>, Body> {
    let https = HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_only()
        .enable_http2()
        .build();
    Client::builder().build(https)
}

trait ReqwestExt {
    fn hyper_request(&self) -> anyhow::Result<hyper::Request<Body>>;
}

impl ReqwestExt for Reqwest<'_> {
    fn hyper_request(&self) -> anyhow::Result<hyper::Request<Body>> {
        // Build headers
        let headers = self.headers()?;

        let method = match self.command.http_verb() {
            HttpMethod::Delete => Method::DELETE,
            HttpMethod::Get => Method::GET,
            HttpMethod::Post => Method::POST,
            HttpMethod::Put => Method::PUT,
            HttpMethod::Head => Method::HEAD,
        };

        let mut request = hyper::Request::builder()
            .method(method)
            .uri(self.url().as_str())
            .body(self.request_body().into())?;
        (*request.headers_mut()) = headers;

        Ok(request)
    }
}

#[async_trait::async_trait]
impl Backend for Bucket {
    async fn read_nar(&self, url: &str) -> Result<NarResponder> {
        let command = Command::GetObject;
        let data_dir = PathBuf::from("data");
        let path = data_dir.join(url);
        let path = path.to_str().ok_or(Error::Upload)?;
        let request = Reqwest::new(self, path, command);
        let request = request.hyper_request().map_err(|_| Error::Download)?;
        let client = https_client();
        let response = client.request(request).await.map_err(|_| Error::Download)?;
        let responder = NarResponder::Stream(response.into_body());
        Ok(responder)
    }
    async fn write_nar(&self, url: &str, reader: &mut DataStream<'_>) -> Result<()> {
        let tmp_dir = PathBuf::from("tmp");
        let path = tmp_dir.join(url);
        let path = path.to_str().ok_or(Error::Upload)?;
        println!("uploading {}", path);
        self.put_object_stream(reader, path).await.map_err(|_| Error::Upload)?;
        Ok(())
    }
    async fn finish_nar(&self, url: &str) -> Result<()> {
        let tmp_dir = PathBuf::from("tmp");
        let data_dir = PathBuf::from("data");
        let tmppath = tmp_dir.join(url);
        let newpath = data_dir.join(tmppath.strip_prefix(&tmp_dir).map_err(|_| Error::Upload)?);
        let tmppath = tmppath.to_str().ok_or(Error::Upload)?;
        let newpath = newpath.to_str().ok_or(Error::Upload)?;
        self.copy_object_internal(tmppath, newpath).await.map_err(|_| Error::Upload)?;
        self.delete_object(tmppath).await.map_err(|_| Error::Upload)?;
        println!("finished {}", newpath);
        Ok(())
    }
}
