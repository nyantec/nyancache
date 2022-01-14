use strum_macros::AsRefStr;
use thiserror::Error as ThisError;

use rocket::http::{ContentType, Status};
use rocket::response::content::Json;
use rocket::response::{Responder, Response};
use rocket::Request;
use std::io::Cursor;

#[derive(ThisError, Debug, AsRefStr)]
pub enum Error {
    #[error("Missing narinfo")]
    MissingNarInfo,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Database(#[from] diesel::result::Error),
    #[error("Upload error")]
    Upload,
    #[error("Download error")]
    Download,
    #[error("Unexpected end of input")]
    UnexpectedEof,
    #[error(transparent)]
    Base64(#[from] base64::DecodeError),
    #[error("Bad base32")]
    BadBase32,
    #[error("Unknown hash type")]
    UnknownHashType,
    #[error("No valid signature")]
    NoValidSignature,
    #[error("Bad narinfo")]
    BadNarInfo,
    #[error("Not found")]
    NotFound,
}

pub type Result<T> = std::result::Result<T, Error>;

impl<'r> Responder<'r, 'r> for Error {
    fn respond_to(self, _: &Request) -> rocket::response::Result<'r> {
        let status = match self {
            Error::NotFound => Status::NotFound,
            _ => Status::InternalServerError,
        };

        let body_json = serde_json::json!({
            "errors": [
                {
                    "status": format!("{}", status.code),
                    "title": status.reason(),
                    "detail": format!("{}", self),
                    "code": self.as_ref(),
                }
            ]
        });

        let body = serde_json::to_string(&body_json).map_err(|_| Status::InternalServerError)?;

        Response::build()
            .header(ContentType::JSON)
            .status(status)
            .sized_body(body.len(), Cursor::new(body))
            .ok()
    }
}
