use axum::body::Bytes;
use axum::extract::Query;
use bson::{doc, from_document, to_document, Document};
use chrono::{Duration, Utc};
use clap::ArgMatches;
use futures::StreamExt;
use hyper::HeaderMap;
use mongodb::options::{FindOptions, IndexOptions};
use mongodb::{Collection, IndexModel};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use blake2::{Digest, Blake2s256};
use hex::encode;
use serde::{Deserialize, Serialize};

use crate::handlers::QueriesSet;
use crate::error::Error as RestError;
use crate::secret::{SecretScrubbed, Secret, SecretPlusData};
use crate::users::{ApiKey, UsersAdmin, ApiKeyBrief};
use crate::auth::CurrentUser;

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub configs: Configs,
    pub mongo_client: mongodb::Client,
    pub users_admin: UsersAdmin,
    pub gcs_client: Arc<cloud_storage::client::Client>,
    pub last_cleanup: Arc<Mutex<i64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keys {
    pub keys: Vec<Key>
}

#[derive(Clone, Debug, Serialize, Deserialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct Key {
    pub ver: u8,
    pub key: String
}

#[derive(Clone, Debug)]
pub struct SecretSaved {
    pub id: String,
    pub key: String,
    pub expire_seconds: i64,
    pub expire_reads: i64,
    pub pwd: bool,
}

#[derive(Clone, Debug)]
pub struct Configs {
    pub url: String,
    pub database: String,
    pub collection_uploads: String,
    pub collection_admin: String,
    pub collection_users: String,
    pub gcs_bucket: String,
    pub keys: Keys
}

impl Keys {
    pub fn latest_key(&self) -> Key {
        self.keys.iter().max_by_key(|x| &x.ver).unwrap().clone()
    }

    pub fn get_ver(&self, ver: u8) -> Option<&Key> {
        self.keys.iter().find(|&v| v.ver == ver)
    }
}

impl State {
    pub async fn new(
        opts: ArgMatches,
        mongo_client: mongodb::Client,
        gcs_client: cloud_storage::client::Client,
    ) -> BoxResult<Self> {
        Ok(State {
            configs: Configs {
                url: opts.value_of("url").unwrap().to_string(),
                database: opts.value_of("database").unwrap().to_string(),
                gcs_bucket: opts.value_of("bucket").unwrap().to_string(),
                collection_uploads: opts.value_of("collection").unwrap().to_string(),
                collection_admin: opts.value_of("admin").unwrap().to_string(),
                collection_users: opts.value_of("users").unwrap().to_string(),
                keys: serde_json::from_str(opts.value_of("keys").unwrap())?
            },
            users_admin: UsersAdmin::new(opts.value_of("database").unwrap(), opts.value_of("users").unwrap(), mongo_client.clone()).await?,
            mongo_client: mongo_client,
            gcs_client: Arc::new(gcs_client),
            last_cleanup: Arc::new(Mutex::new(Utc::now().timestamp())),
        })
    }

    pub fn collection(&self) -> Collection<Document> {
        self.mongo_client
            .database(&self.configs.database)
            .collection(&self.configs.collection_uploads)
    }

    pub fn admin(&self) -> Collection<Document> {
        self.mongo_client
            .database(&self.configs.database)
            .collection(&self.configs.collection_admin)
    }

    pub async fn increment(&self, id: &str) -> Result<Secret, RestError> {
        log::debug!("Attempting to increment hit counter on {}", id);
        let filter = doc! {"id": id, "active": true};
        let update = doc! { "$inc": { "lifecycle.current.reads": 1 } };
        match self
            .collection()
            .find_one_and_update(filter, update, None)
            .await
        {
            Ok(v) => match v {
                Some(v) => Ok(from_document(v)?),
                None => {
                    log::debug!("Could not get {} from mongo", id);
                    Err(RestError::NotFound)
                }
            },
            Err(e) => {
                log::error!("Error updating for {}: {}", id, e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn fetch_doc(&self, id: &str) -> Result<Secret, RestError> {
        let filter = doc! {"links.id": id, "active": true };
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

    pub async fn fetch_object(&self, id: &str) -> Result<Vec<u8>, RestError> {
        log::debug!("Downloading {} from bucket", id);
        // Get value from bucket
        match self
            .gcs_client
            .object()
            .download(&self.configs.gcs_bucket, id)
            .await
        {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("\"Got error attempting to fetch id from GCS: {}\"", e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn delete_object(&self, id: &str) -> Result<(), RestError> {
        // Delete value from bucket
        match self.gcs_client.object().delete(&self.configs.gcs_bucket, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("\"Got error attempting to fetch id from GCS: {}\"", e);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn delete(&self, id: &str) -> Result<(), RestError> {
        log::debug!("\"Deleting {}\"", &id);

        let filter = doc! {"id": id, "active": true};
        let update = doc! {"$set": {"active": false }};

        // Set doc to not active
        match self
            .collection()
            .find_one_and_update(filter, update, None)
            .await?
        {
            Some(_) => log::debug!("{} set to active=false", id),
            None => {
                log::warn!("Could not find {} to update active=false", id);
                return Err(RestError::NotFound);
            }
        }

        // Delete object
        self.delete_object(id).await?;

        Ok(())
    }

    pub async fn get(
        &mut self,
        id: &str,
        key: &str,
        password: Option<&String>,
    ) -> Result<(Vec<u8>, String), RestError> {
        // Kick off cleanup
        self.cleanup().await?;

        // Get doc from mongo
        let secret = self.fetch_doc(id).await?;

        // Compare password hash
        if let Some(hash) = secret.facts.pwd {
            match password {
                Some(p) => {
                    let mut hasher = Blake2s256::new();
                    hasher.update(p.as_bytes());
                    let password_hash = encode(hasher.finalize());
                    if password_hash != hash {
                        log::warn!("\"Note requested didn't match required password\"");
                        return Err(RestError::NotFound);
                    }
                }
                None => {
                    log::warn!(
                        "\"Password protected Note requested without providing a password\""
                    );
                    return Err(RestError::NotFound);
                }
            }
        }

        // If encryption is managed, check client key against link key
        if secret.facts.encryption.managed {
            if let Some(link) = secret.links.get(id) {
                // This should not error
                if link.key.is_none() {
                    return Err(RestError::NotFound);
                }
                let mut hasher = Blake2s256::new();
                hasher.update(key.as_bytes());
                let client_key_hash = encode(hasher.finalize());

                if &client_key_hash != link.key.as_ref().unwrap() {
                    log::warn!("\"Client key did not match link key\"");
                    return Err(RestError::NotFound);
                }
            } else {
                log::error!("Mongo returned doc that did not have matching key");
                return Err(RestError::NotFound);
            }
        }

        // If key is expired, delete
        if Utc::now().timestamp_millis() > secret.lifecycle.max.expires.timestamp_millis() {
            log::debug!("\"Key has expired: {}\"", key);
            return Err(RestError::NotFound);
        }

        // Get data from storage
        let value = self.fetch_object(&secret.id).await?;

        // Get decryption key, either from the mongo doc, or from the client
        let decryption_key = if !secret.facts.encryption.managed {
            log::debug!("Using client-provided decryption key");
            key.to_owned()
        } else {
            let decrypt_key_ver = secret.facts.encryption.version.expect("Missing requiered encryption key version");
            let decrypt_key = self.configs.keys.get_ver(decrypt_key_ver).expect("error getting decryption key version from mongodoc");
            let encrypted_key = secret.facts.encryption.key.expect("Missing requiered encryption key");

            // Decrypt encryption key
            let secret_key = orion::aead::SecretKey::from_slice(decrypt_key.key.as_bytes())?;
            match orion::aead::open(&secret_key, &encrypted_key) {
                Ok(e) => {
                    let key = std::str::from_utf8(&e)?;
                    key.to_owned()
                },
                Err(e) => {
                    log::error!("\"Error decrypting encryption key: {}\"", e);
                    return Err(RestError::NotFound);
                }
            }
        };

        // Decrypt data
        let secret_key = orion::aead::SecretKey::from_slice(decryption_key.as_bytes())?;
        let value = match orion::aead::open(&secret_key, &value) {
            Ok(e) => e,
            Err(e) => {
                log::error!("\"Error decrypting secret: {}\"", e);
                return Err(RestError::NotFound);
            }
        };

        // If key has been accessed the max number of times, then remove
        if secret.lifecycle.max.reads > 0
            && secret.lifecycle.current.reads + 1 >= secret.lifecycle.max.reads
        {
//            self.increment(&secret.id).await?;
            self.delete(&secret.id).await?;
            log::debug!("Preemptively deleting id, max expire_reads reached");
        } else {
            // Increment hit count
            self.increment(&secret.id).await?;
        };

        Ok((value, secret.meta.content_type))
    }

    pub async fn set(
        &mut self,
        value: Bytes,
        queries: &Query<QueriesSet>,
        headers: HeaderMap,
        current_user: CurrentUser
    ) -> Result<SecretSaved, RestError> {
        // Create new secret from data
        let secretplusdata =
            Secret::create(value, queries, headers, current_user.id, &self.configs.keys)?;

        let key = secretplusdata.key.clone();
        let expire_seconds = secretplusdata.secret.lifecycle.max.seconds;
        let expire_reads = secretplusdata.secret.lifecycle.max.reads;

        let id = self.insert(secretplusdata).await?;
        log::debug!(
            "\"Saved with expiration of {} seconds, and {} max expire_reads\"",
            expire_seconds,
            expire_reads
        );
        Ok(SecretSaved {
            id,
            key: key.to_string(),
            expire_seconds,
            expire_reads,
            pwd: queries.pwd.is_some(),
        })
    }

    pub async fn insert(&mut self, secret_plus_key: SecretPlusData) -> Result<String, RestError> {
        log::debug!("inserting doc into mongo");
        let bson = to_document(&secret_plus_key.secret)?;

        self.gcs_client
            .object()
            .create(
                &self.configs.gcs_bucket,
                secret_plus_key.value,
                &secret_plus_key.secret.id,
                &secret_plus_key.secret.meta.content_type,
            )
            .await?;

        match self.collection().insert_one(bson, None).await {
            Ok(_) => Ok(secret_plus_key.secret.links.first().unwrap().id.to_owned()),
            Err(e) => {
                log::error!("Error updating for {}: {}", secret_plus_key.secret.id, e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn create_uploads_indexes(&mut self) -> Result<(), RestError> {
        log::debug!("Creating upload collection indexes");
        let mut indexes = Vec::new();
        indexes.push(
            IndexModel::builder()
                .keys(doc! {"id":1, "active": 1})
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        );

        indexes.push(
            IndexModel::builder()
                .keys(doc! {"active":1, "lifecycle.expires_at": 1})
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
        // let options = FindOneAndUpdateOptions::builder().upsert(true).build();

        // Check if cleanup doc already exists, and create it if it does not
        if self
            .admin()
            .find_one(filter_lock.clone(), None)
            .await?
            .is_none()
        {
            log::debug!("Cleanup lock doc does not exist, creating");
            let cleanup_doc = doc! {"name":"cleanup", "active": false, "modified": Utc::now() };
            self.admin().insert_one(cleanup_doc, None).await?;
            return Ok(());
        }

        // Ensure cleanup doc is not in a "failed" state
        let filter_lock =
            doc! {"name":"cleanup", "active": true, "modified": { "$lt" : Utc::now() - Duration::minutes(5) } };
        let update_lock = doc! {"$set": {"active": false, "modified": Utc::now() }};
        match self
            .admin()
            .find_one_and_update(filter_lock, update_lock, None)
            .await?
        {
            Some(_) => log::debug!("Reset cleanup doc modification time"),
            None => log::debug!("Cleanup doc already is correct"),
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
        let query = doc! {"active": true, "lifecycle.max.expires": {"$lt": Utc::now()}};
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
                    log::debug!(
                        "\"{} queued to be deleted\"",
                        doc.get_str("id")?.to_string()
                    );
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

    pub async fn uploads_owned(&self, id: &str) -> Result<Vec<SecretScrubbed>, RestError> {
        let query = doc! {"active": true, "facts.owner": id, "lifecycle.max.expires": {"$gt": Utc::now()}};
        let find_options = FindOptions::builder()
            .sort(doc! { "_id": -1 })
            .limit(1000)
            .build();

        let coll = self.mongo_client.database(&self.configs.database).collection::<Secret>(&self.configs.collection_uploads);
        let mut cursor = coll.find(query, find_options).await?;
        let mut result: Vec<SecretScrubbed> = Vec::new();
        while let Some(document) = cursor.next().await {
            match document {
                Ok(doc) => {
                    result.push(doc.to_json())
                }
                Err(e) => {
                    log::error!("Caught error, skipping: {}", e);
                    continue;
                }
            }
        }
        Ok(result)
    }

    pub async fn cleanup_work(&self) -> Result<(), RestError> {
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
        // Only perform cleanup if internal timeout has breached 60 seconds
        if self.lock_timer().await.is_err() {
            return Ok(());
        }

        // Lock cleanup doc
        if self.lock_cleanup().await.is_err() {
            return Ok(());
        }

        self.cleanup_thread().await?;
        Ok(())
    }

    pub async fn cleanup_init(&self) -> Result<(), RestError> {
        // Lock cleanup doc, without internal ticker check
        if self.lock_cleanup().await.is_err() {
            return Ok(());
        }

        self.cleanup_thread().await?;
        Ok(())
    }

    pub async fn cleanup_thread(&self) -> Result<(), RestError> {
        // Send actual work to background thread
        let me = self.clone();
        tokio::spawn(async move {
            log::debug!("Kicking off background thread to perform cleanup");
            me.cleanup_work().await
        });
        Ok(())
    }

    pub async fn init(&mut self) -> Result<(), RestError> {
        self.admin_init().await?;
        self.cleanup_init().await?;
        self.create_uploads_indexes().await?;
        Ok(())
    }

    pub async fn create_user(&self, email: &str, pwd: &str) -> Result<String, RestError> {
        self.users_admin.create_user(email, pwd).await
    }

    pub async fn get_user_id(&self, email: &str, pwd: &str) -> Result<String, RestError> {
        self.users_admin.get_user_id(email, pwd).await
    }

    pub async fn create_api_key(&self, id: &str) -> Result<ApiKey, RestError> {
        self.users_admin.create_api_key(id).await
    }

    pub async fn list_api_keys(&self, id: &str) -> Result<Vec<ApiKeyBrief>, RestError> {
        match self.users_admin.list_api_keys(id).await {
            Ok(u) => Ok(u),
            Err(e) => Err(e)
        }
    }

    pub async fn delete_api_key(&self, id: &str, key: &str) -> Result<bool, RestError> {
        self.users_admin.delete_api_key(id, key).await
    }
}
