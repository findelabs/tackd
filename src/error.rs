//use serde_json::error::Error as SerdeError;
use axum::{
    body::{self},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::fmt;

#[derive(Debug)]
pub enum Error {
    NotFound,
    CryptoError(orion::errors::UnknownCryptoError),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::NotFound => f.write_str("{\"error\": \"Not found\"}"),
            Error::CryptoError(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let payload = self.to_string();
        let body = body::boxed(body::Full::from(payload));

        let status_code = match self {
            Error::NotFound => StatusCode::NOT_FOUND,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Response::builder().status(status_code).body(body).unwrap()
    }
}

impl From<orion::errors::UnknownCryptoError> for Error {
    fn from(err: orion::errors::UnknownCryptoError) -> Error {
        Error::CryptoError(err)
    }
}
