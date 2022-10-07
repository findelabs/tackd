use clap::ArgMatches;
use std::error::Error;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use chrono::Utc;
use axum::body::Bytes;
use orion::pwhash;
use uuid::Uuid;

//use crate::https::{HttpsClient, ClientBuilder};
use crate::error::Error as RestError;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct LockBox {
    inner: Arc<RwLock<HashMap<String, Secret>>>
}

#[derive(Clone, Debug)]
pub struct Secret {
    created: i64,
    hits: u64,
    expires: Option<u64>,
    reads: Option<u64>,
    value: Vec<u8>
}

#[derive(Clone, Debug)]
pub struct SecretPlusKey {
    secret: Secret,
    key: String
}

#[derive(Clone, Debug)]
pub struct IdPlusKey {
    pub id: String,
    pub key: String
}

impl Secret {
    pub fn create(value: String, reads: Option<u64>, expires: Option<u64>) -> Result<SecretPlusKey, RestError> {
        let key = Uuid::new_v4();
        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;
        let ciphertext = match orion::aead::seal(&secret_key, &value.as_bytes()) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e))
            }
        };
        println!("here");
        let secret = Secret {
            created: Utc::now().timestamp(),
            hits: 0u64,
            expires,
            reads,
            value: ciphertext
        };

        Ok(SecretPlusKey {
            secret,
            key: key.to_string()
        })
    }
}

impl LockBox {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn set(&mut self, value: String, reads: Option<u64>, expires: Option<u64>) -> Result<IdPlusKey, RestError> {
        let secret_plus_key = Secret::create(value, reads, expires)?;
        let key = secret_plus_key.key.clone();

        let id = self.insert(secret_plus_key).await.expect("Failed to insert key");

        Ok(IdPlusKey {
            id: id.to_string(),
            key: key.to_string()
        })
    }

    pub async fn insert(&mut self, secret_plus_key: SecretPlusKey) -> Option<Uuid> {
        let id = Uuid::new_v4();
        let mut lock = self.inner.write().await;
        match lock.insert(id.to_string(), secret_plus_key.secret) {
            Some(_) => Some(id),
            None => None
        }
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
