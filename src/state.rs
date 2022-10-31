use axum::body::Bytes;
use bson::doc;
use bson::from_document;
use bson::to_document;
use bson::Document;
use chrono::Utc;
use clap::ArgMatches;
use mongodb::Collection;
use mongodb::IndexModel;
use mongodb::options::IndexOptions;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::error::Error;
use uuid::Uuid;
use chrono::Duration;

//use crate::https::{HttpsClient, ClientBuilder};
use crate::error::Error as RestError;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub url: String,
    pub database: String,
    pub collection: String,
    pub client: mongodb::Client,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Secret {
    created: i64,
    expires_at: bson::DateTime,
    id: String,
    content_type: String,
    hits: i64,
    expire_seconds: i64,
    expire_reads: Option<i64>,
    #[serde(with = "serde_bytes")]
    value: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SecretInfo {
    secret: Secret,
    key: String,
}

#[derive(Clone, Debug)]
pub struct SecretSaved {
    pub id: String,
    pub key: String,
    pub expire_seconds: i64,
    pub expire_reads: Option<i64>,
}

impl Secret {
    pub fn create(
        value: Bytes,
        expire_reads: Option<i64>,
        expire_seconds: Option<i64>,
        content_type: String,
    ) -> Result<SecretInfo, RestError> {
        log::debug!("Sealing up {:?}", &value);

        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

        log::debug!("Secret key: {}", key);
        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;
        let ciphertext = match orion::aead::seal(&secret_key, &value) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e));
            }
        };

        // Ensure max expire_seconds is less than a month
        let expire_seconds = match expire_seconds {
            Some(v) => {
                if v > 2592000i64 {
                    log::warn!("Incorrect expire_seconds found, dropping to 2,592,000");
                    2592000i64
                } else {
                    v
                }
            },
            None => {
                log::debug!("No expiration set, defaulting to one month");
                2592000i64
            }
        };

        let expire_reads = match expire_reads {
            Some(t) => Some(t),
            None => Some(1)
        };

        let expires_at = Utc::now() + Duration::seconds(expire_seconds);

        let secret = Secret {
            created: Utc::now().timestamp(),
            expires_at: expires_at.into(), 
            id: Uuid::new_v4().to_string(),
            content_type,
            hits: 0i64,
            expire_seconds,
            expire_reads,
            value: ciphertext,
        };

        Ok(SecretInfo { secret, key })
    }
}

impl State {
    pub async fn new(opts: ArgMatches, client: mongodb::Client) -> BoxResult<Self> {
        Ok(State {
            url: opts.value_of("url").unwrap().to_string(),
            database: opts.value_of("database").unwrap().to_string(),
            collection: opts.value_of("collection").unwrap().to_string(),
            client,
        })
    }

    pub fn collection(&self) -> Collection<Document> {
        self.client
            .database(&self.database)
            .collection(&self.collection)
    }

    pub async fn increment(&self, id: &str) -> Result<Secret, RestError> {
        let filter = doc! {"id": id};
        let update = doc! { "$inc": { "hits": 1 } };
        match self
            .collection()
            .find_one_and_update(filter, update, None)
            .await
        {
            Ok(v) => match v {
                Some(v) => Ok(from_document(v)?),
                None => Err(RestError::NotFound),
            },
            Err(e) => {
                log::error!("Error updating for {}: {}", id, e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn fetch(&self, id: &str) -> Result<Secret, RestError> {
        let filter = doc! {"id": id};
        match self.collection().find_one(Some(filter), None).await {
            Ok(v) => match v {
                Some(v) => Ok(from_document(v)?),
                None => Err(RestError::NotFound),
            },
            Err(e) => {
                log::error!("Error searching for {}: {}", id, e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), RestError> {
        let filter = doc! {"id": id};
        if let Err(e) = self.collection().delete_one(filter, None).await {
            log::error!("Error deleting for {}: {}", id, e);
            return Err(RestError::NotFound);
        }
        Ok(())
    }

    pub async fn get(&mut self, id: &str, key: &str) -> Result<(Vec<u8>, String), RestError> {
        let secret = self.fetch(id).await?;

        // If key is expired, delete
        if Utc::now().timestamp() > secret.created + secret.expire_seconds as i64 {
            log::debug!("\"Key has expired: {}\"", key);
            self.delete(id).await?;
            return Err(RestError::NotFound);
        }

        // If key has been accessed the max number of times, then remove
        if let Some(expire_reads) = secret.expire_reads {
            if secret.hits + 1 >= expire_reads {
                self.delete(id).await?;
                log::debug!("Preemptively deleting id, max expire_reads reached");
            } else {
                // Increment hit count
                self.increment(id).await?;
            }
        };

        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;
        let value = match orion::aead::open(&secret_key, &secret.value) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error decrypting secret: {}", e);
                return Err(RestError::NotFound);
            }
        };

        Ok((value, secret.content_type))
    }

    pub async fn set(
        &mut self,
        value: Bytes,
        expire_reads: Option<i64>,
        expire_seconds: Option<i64>,
        content_type: String,
    ) -> Result<SecretSaved, RestError> {
        let secret = Secret::create(value, expire_reads, expire_seconds, content_type)?;
        let key = secret.key.clone();
        let expire_seconds = secret.secret.expire_seconds;
        let expire_reads = secret.secret.expire_reads;

        let id = self.insert(secret).await?;
        log::debug!(
            "\"Saved with expiration of {} seconds, and {} max expire_reads\"",
            expire_seconds,
            expire_reads.unwrap_or(i64::MAX)
        );
        Ok(SecretSaved {
            id: id.to_string(),
            key: key.to_string(),
            expire_seconds,
            expire_reads,
        })
    }

    pub async fn insert(&mut self, secret_plus_key: SecretInfo) -> Result<String, RestError> {
        log::debug!("inserting key");
        let bson = to_document(&secret_plus_key.secret)?;

        match self.collection().insert_one(bson, None).await {
            Ok(_) => Ok(secret_plus_key.secret.id),
            Err(e) => {
                log::error!("Error updating for {}: {}", secret_plus_key.secret.id, e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn create_indexes(&mut self) -> Result<(), RestError> {
        log::debug!("Creating indexes");

        let mut indexes = Vec::new();

        indexes.push(IndexModel::builder().keys(doc! {"id":1}).build());
        indexes.push(IndexModel::builder().keys(doc! {"expires_at":1}).options(IndexOptions::builder().expire_after(Some(std::time::Duration::from_secs(0))).build()).build());

        match self.collection().create_indexes(indexes, None).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("Error creating index: {}", e);
                Err(RestError::BadInsert)
            }
        }
    }
}
