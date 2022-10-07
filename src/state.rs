use clap::ArgMatches;
use std::error::Error;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use chrono::Utc;
use axum::body::Bytes;

//use crate::https::{HttpsClient, ClientBuilder};
//use crate::error::Error as RestError;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct LockBox {
    inner: Arc<RwLock<HashMap<String, Secret>>>
}

#[derive(Clone, Debug)]
pub struct Secret {
    created: Utc,
    expires: u64,
    hits: u64,
    reads: u64,
    value: Bytes
}

impl Secret {
    pub fn create(value: String) -> Result<Vec<u8>, UnknownCryptoError> {
        todo!()
    }
}

impl LockBox {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[derive(Clone, Debug)]
pub struct State {
    pub lock: LockBox
}

impl State {
    pub async fn new(opts: ArgMatches) -> BoxResult<Self> {
        Ok(State {
            lock: LockBox::new()
        })
    }
}
