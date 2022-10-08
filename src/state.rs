use clap::ArgMatches;
use std::error::Error;
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::Arc;
use chrono::Utc;
use uuid::Uuid;
use rand::distributions::{Alphanumeric, DistString};


//use crate::https::{HttpsClient, ClientBuilder};
use crate::error::Error as RestError;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct LockBox {
    inner: Arc<RwLock<HashMap<String, Secret>>>
}

#[derive(Clone, Debug)]
pub struct Secret {
    inner: Arc<RwLock<SecretInner>>
}

#[derive(Clone, Debug)]
pub struct SecretInner {
    created: i64,
    hits: u64,
    expires: Option<u64>,
    reads: Option<u64>,
    value: Vec<u8>
}

#[derive(Clone, Debug)]
pub struct SecretInfo {
    secret: Secret,
    key: String
}

#[derive(Clone, Debug)]
pub struct SecretSaved {
    pub id: String,
    pub key: String,
    pub expires: Option<u64>,
    pub reads: Option<u64>
}

impl Secret {
    pub async fn created(&self) -> i64 {
        let lock = self.inner.read().await;
        lock.created
    }

    pub async fn hits(&self) -> u64 {
        let lock = self.inner.read().await;
        lock.hits
    }

    pub async fn increment(&mut self) -> u64 {
        let mut lock = self.inner.write().await;
        lock.hits += 1;
        lock.hits
    }

    pub async fn reads(&self) -> Option<u64> {
        let lock = self.inner.read().await;
        lock.reads
    }

    pub async fn expires(&self) -> Option<u64> {
        let lock = self.inner.read().await;
        lock.expires
    }

    pub async fn value(&self) -> Vec<u8> {
        let lock = self.inner.read().await;
        lock.value.clone()
    }

    pub fn create(value: String, reads: Option<u64>, expires: Option<u64>) -> Result<SecretInfo, RestError> {
        log::debug!("Sealing up {}", &value);

        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

        log::debug!("Secret key: {}", key);
        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;
        let ciphertext = match orion::aead::seal(&secret_key, value.as_bytes()) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e))
            }
        };

        let reads = match reads {
            Some(r) => Some(r),
            None => Some(1u64)
        };

        let expires = match expires {
            Some(r) => Some(r),
            None => Some(600u64)
        };

        let secret_inner = SecretInner {
            created: Utc::now().timestamp(),
            hits: 0u64,
            expires,
            reads,
            value: ciphertext
        };

        let secret = Secret { inner: Arc::new(RwLock::new(secret_inner)) };

        Ok(SecretInfo{
            secret,
            key
        })
    }
}

impl LockBox {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn fetch(&self, id: &str) -> Result<Secret, RestError> {
        let mut lock = self.inner.write().await;
        match lock.get_mut(id) {
            Some(v) => Ok(v.clone()),
            None => Err(RestError::NotFound)
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), RestError> {
        let mut lock = self.inner.write().await;
        lock.remove(id);
        Ok(())
    }

    pub async fn get(&mut self, id: &str, key: &str) -> Result<Vec<u8>, RestError> {
        let mut secret = self.fetch(id).await?;

        // If key is expired, delete
        if let Some(expires) = secret.expires().await {
            if Utc::now().timestamp() > secret.created().await + expires as i64 {
                log::debug!("\"Key has expired: {}\"", key);
                self.delete(id).await?;
                return Err(RestError::NotFound)
            }
        };

        // If key has been accessed the max number of times, then remove
        if let Some(reads) = secret.reads().await {
            if secret.hits().await >= reads {
                self.delete(id).await?;
                return Err(RestError::NotFound)
            }
        };

        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;
        let value = match orion::aead::open(&secret_key, &secret.value().await) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error decrypting secret: {}", e);
                return Err(RestError::NotFound)
            }
        };

        // Increment hit count
        let hits = secret.increment().await;
        log::debug!("\"incrementing hit count to {}", hits);

        Ok(value)
    }

    pub async fn set(&mut self, value: String, reads: Option<u64>, expires: Option<u64>) -> Result<SecretSaved, RestError> {
        let secret = Secret::create(value, reads, expires)?;
        let key = secret.key.clone();

        let id = self.insert(secret).await;

        Ok(SecretSaved{
            id: id.to_string(),
            key: key.to_string(),
            expires,
            reads
        })
    }

    pub async fn insert(&mut self, secret_plus_key: SecretInfo) -> Uuid {
        log::debug!("inserting key");
        let id = Uuid::new_v4();
        // Check to see if uuid already exists here
        let mut lock = self.inner.write().await;
        lock.insert(id.to_string(), secret_plus_key.secret);
        id
    }
}

#[derive(Clone, Debug)]
pub struct State {
    pub url: String,
    pub lock: LockBox
}

impl State {
    pub async fn new(opts: ArgMatches) -> BoxResult<Self> {
        Ok(State {
            url: opts.value_of("url").unwrap().to_string(),
            lock: LockBox::new()
        })
    }
}
