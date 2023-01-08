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
    BadInsert,
    CleanupNotRequired,
    UserExists,
    BadLogin,
    Unauthorized,
    CryptoError(orion::errors::UnknownCryptoError),
    DeError(bson::de::Error),
    SerError(bson::ser::Error),
    Mongo(mongodb::error::Error),
    Storage(cloud_storage::Error),
    Bson(bson::document::ValueAccessError),
    Utf(std::str::Utf8Error),
    Azure(azure_core::error::Error),
    Ms(ms_converter::Error),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Error::NotFound => f.write_str("{\"error\": \"Not found\"}"),
            Error::BadInsert => f.write_str("{\"error\": \"Unable to insert metadata\"}"),
            Error::CleanupNotRequired => {
                f.write_str("{\"error\": \"Cleanup not required at this time\"}")
            }
            Error::UserExists => f.write_str("{\"error\": \"User already exists\"}"),
            Error::BadLogin => f.write_str("{\"error\": \"Incorrect login credentials\"}"),
            Error::Unauthorized => f.write_str("{\"error\": \"Unauthorized\"}"),
            Error::CryptoError(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::DeError(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::SerError(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::Mongo(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::Storage(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::Bson(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::Utf(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::Azure(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
            Error::Ms(ref err) => write!(f, "{{\"error\": \"{}\"}}", err),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let payload = self.to_string();
        let body = body::boxed(body::Full::from(payload));

        let status_code = match self {
            Error::NotFound => StatusCode::NOT_FOUND,
            Error::UserExists => StatusCode::CONFLICT,
            Error::BadLogin | Error::Unauthorized => StatusCode::UNAUTHORIZED,
            Error::DeError(_) => StatusCode::NOT_FOUND,
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

impl From<bson::de::Error> for Error {
    fn from(err: bson::de::Error) -> Error {
        Error::DeError(err)
    }
}

impl From<bson::ser::Error> for Error {
    fn from(err: bson::ser::Error) -> Error {
        Error::SerError(err)
    }
}

impl From<mongodb::error::Error> for Error {
    fn from(err: mongodb::error::Error) -> Error {
        Error::Mongo(err)
    }
}

impl From<cloud_storage::Error> for Error {
    fn from(err: cloud_storage::Error) -> Error {
        Error::Storage(err)
    }
}

impl From<bson::document::ValueAccessError> for Error {
    fn from(err: bson::document::ValueAccessError) -> Error {
        Error::Bson(err)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Error {
        Error::Utf(err)
    }
}

impl From<azure_core::error::Error> for Error {
    fn from(err: azure_core::error::Error) -> Error {
        Error::Azure(err)
    }
}

impl From<ms_converter::Error> for Error {
    fn from(err: ms_converter::Error) -> Error {
        Error::Ms(err)
    }
}
