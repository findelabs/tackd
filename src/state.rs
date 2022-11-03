use axum::body::Bytes;
use bson::doc;
use bson::from_document;
use bson::to_document;
use bson::Document;
use chrono::Duration;
use chrono::Utc;
use clap::ArgMatches;
use futures::StreamExt;
use mongodb::options::FindOneAndUpdateOptions;
use mongodb::options::FindOptions;
use mongodb::options::IndexOptions;
use mongodb::Collection;
use mongodb::IndexModel;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error::Error as RestError;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub url: String,
    pub database: String,
    pub collection: String,
    pub collection_admin: String,
    pub mongo_client: mongodb::Client,
    pub gcs_client: Arc<cloud_storage::client::Client>,
    pub gcs_bucket: String,
    pub last_cleanup: Arc<Mutex<i64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Secret {
    expires_at: bson::DateTime,
    id: String,
    content_type: String,
    hits: i64,
    expire_seconds: i64,
    expire_reads: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct SecretInfo {
    secret: Secret,
    key: String,
    value: Vec<u8>,
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

        let expire_reads = if expire_seconds.is_none() && expire_reads.is_none() {
            Some(1)
        } else {
            expire_reads
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
            }
            None => {
                log::debug!("No expiration set, defaulting to one hour");
                3600
            }
        };

        let expires_at = Utc::now() + Duration::seconds(expire_seconds);

        let secret = Secret {
            expires_at: expires_at.into(),
            id: Uuid::new_v4().to_string(),
            content_type,
            hits: 0i64,
            expire_seconds,
            expire_reads,
        };

        Ok(SecretInfo {
            secret,
            key,
            value: ciphertext,
        })
    }
}

impl State {
    pub async fn new(
        opts: ArgMatches,
        mongo_client: mongodb::Client,
        gcs_client: cloud_storage::client::Client,
    ) -> BoxResult<Self> {
        Ok(State {
            url: opts.value_of("url").unwrap().to_string(),
            database: opts.value_of("database").unwrap().to_string(),
            gcs_bucket: opts.value_of("bucket").unwrap().to_string(),
            collection: opts.value_of("collection").unwrap().to_string(),
            collection_admin: opts.value_of("admin").unwrap().to_string(),
            mongo_client,
            gcs_client: Arc::new(gcs_client),
            last_cleanup: Arc::new(Mutex::new(Utc::now().timestamp())),
        })
    }

    pub fn collection(&self) -> Collection<Document> {
        self.mongo_client
            .database(&self.database)
            .collection(&self.collection)
    }

    pub fn admin(&self) -> Collection<Document> {
        self.mongo_client
            .database(&self.database)
            .collection(&self.collection_admin)
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

    pub async fn fetch_doc(&self, id: &str) -> Result<Secret, RestError> {
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

    pub async fn fetch_id(&self, id: &str) -> Result<Vec<u8>, RestError> {
        // Get value from bucket
        match self
            .gcs_client
            .object()
            .download(&self.gcs_bucket, &id)
            .await
        {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("\"Got error attempting to fetch id from GCS: {}\"", e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn delete_id(&self, id: &str) -> Result<(), RestError> {
        // Delete value from bucket
        match self.gcs_client.object().delete(&self.gcs_bucket, &id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("\"Got error attempting to fetch id from GCS: {}\"", e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), RestError> {
        log::debug!("\"Deleting {}\"", &id);
        let filter = doc! {"id": id};
        if let Err(e) = self.collection().delete_one(filter, None).await {
            log::error!("Error deleting for {}: {}", id, e);
            return Err(RestError::NotFound);
        }

        self.delete_id(&id).await?;

        Ok(())
    }

    pub async fn get(&mut self, id: &str, key: &str) -> Result<(Vec<u8>, String), RestError> {
        // Kick off cleanup
        self.cleanup().await?;

        let secret = self.fetch_doc(id).await?;

        // If key is expired, delete
        if Utc::now().timestamp_millis() > secret.expires_at.timestamp_millis() {
            log::debug!("\"Key has expired: {}\"", key);
            return Err(RestError::NotFound);
        }

        let value = self.fetch_id(&id).await?;

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
        let value = match orion::aead::open(&secret_key, &value) {
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

        self.gcs_client
            .object()
            .create(
                &self.gcs_bucket,
                secret_plus_key.value,
                &secret_plus_key.secret.id,
                &secret_plus_key.secret.content_type,
            )
            .await?;
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
        indexes.push(
            IndexModel::builder()
                .keys(doc! {"id":1})
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        );

        match self.collection().create_indexes(indexes, None).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("Error creating index: {}", e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn admin_init(&self) -> Result<(), RestError> {
        // Create cleanup lock
        let filter_lock = doc! {"name":"cleanup"};
        let update_lock = doc! {"$set": {"active": false, "modified": Utc::now()}};
        let options = FindOneAndUpdateOptions::builder().upsert(true).build();

        // Create cleanup doc if it does not exist
        match self
            .admin()
            .find_one_and_update(filter_lock, update_lock, Some(options))
            .await?
        {
            Some(_) => log::debug!("Cleanup doc already existed"),
            None => log::debug!("Created cleanup doc"),
        }
        Ok(())
    }

    pub async fn lock_timer(&self) -> Result<(), RestError> {
        let mut last_cleanup = self.last_cleanup.lock().await;
        let now = Utc::now().timestamp();
        if now > *last_cleanup + 60 {
            *last_cleanup = now;
            Ok(())
        } else {
            log::debug!("\"Cleanup skipped, internal timer not at 60 seconds\"");
            Err(RestError::CleanupNotRequired)
        }
    }

    pub async fn lock_cleanup(&self) -> Result<(), RestError> {
        // Only perform cleanup if internal timeout has breached 60 seconds
        self.lock_timer().await?;

        log::debug!("\"Attempting to lock cleanup doc\"");
        let delay = Utc::now() - Duration::seconds(60);
        let filter_lock = doc! {"active": false, "name":"cleanup", "modified": {"$lt": delay }};
        let update_lock = doc! {"$set": {"active": true, "modified": Utc::now()}};

        // Try to lock cleanup doc
        match self
            .admin()
            .find_one_and_update(filter_lock, update_lock, None)
            .await?
        {
            Some(_) => log::debug!("\"Locked cleanup doc\""),
            None => {
                log::debug!("\"Cleanup not required at this time\"");
                return Err(RestError::CleanupNotRequired);
            }
        };
        Ok(())
    }

    pub async fn unlock_cleanup(&self) -> Result<(), RestError> {
        log::debug!("\"Freeing cleanup doc\"");
        let filter_unlock = doc! {"active": true, "name":"cleanup"};
        let update_unlock = doc! {"$set": {"active": false}};

        // Unlock cleanup doc
        match self
            .admin()
            .find_one_and_update(filter_unlock, update_unlock, None)
            .await?
        {
            Some(_) => log::debug!("\"Freed up cleanup doc\""),
            None => log::error!("Unable to free cleanup doc"),
        };
        Ok(())
    }

    pub async fn expired_ids(&self) -> Result<Vec<String>, RestError> {
        // Search for docs that are expired here
        let query = doc! {"expires_at": {"$lt": Utc::now()}};
        let find_options = FindOptions::builder()
            .sort(doc! { "_id": -1 })
            .projection(doc! {"id":1, "_id":0})
            .limit(1000)
            .build();

        let mut cursor = self.collection().find(query, find_options).await?;
        let mut result: Vec<String> = Vec::new();
        while let Some(document) = cursor.next().await {
            match document {
                Ok(doc) => {
                    log::debug!("\"{} queued to be deleted\"", doc.get_str("id")?.to_string());
                    result.push(doc.get_str("id")?.to_string())
                }
                Err(e) => {
                    log::error!("Caught error, skipping: {}", e);
                    continue;
                }
            }
        }
        Ok(result)
    }

    pub async fn cleanup_thread(&self) -> Result<(), RestError> {
        // Get expired ids
        let ids = self.expired_ids().await?;

        for id in ids {
            self.delete(&id.to_string()).await?;
        }

        // Unlock the cleanup doc
        self.unlock_cleanup().await?;

        Ok(())
    }

    pub async fn cleanup(&self) -> Result<(), RestError> {
        // Lock cleanup doc
        if let Err(_) = self.lock_cleanup().await {
            return Ok(());
        }

        // Send actual work to background thread
        let me = self.clone();
        tokio::spawn(async move {
            log::debug!("Kicking off background thread to perform cleanup");
            me.cleanup_thread().await
        });
        Ok(())
    }
}
