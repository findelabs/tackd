use axum::body::Bytes;
use axum::extract::Query;
use blake2::{Blake2s256, Digest};
use bson::{doc, to_document, Document};
use chrono::{Duration, Utc};
use clap::ArgMatches;
use hex::encode;
use hyper::HeaderMap;
use mongodb::options::{FindOptions, IndexOptions};
use mongodb::IndexModel;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

use crate::database::links::{Link, LinkScrubbed, LinkWithKey};
use crate::database::mongo::MongoClient;
//use crate::database::secret::{Secret};
use crate::database::metadata::{MetaData, MetaDataPayload, MetaDataPublic};
use crate::database::users::{ApiKey, ApiKeyBrief, CurrentUser, UsersAdmin};
use crate::error::Error as RestError;
use crate::handlers::QueriesSet;
use crate::storage::trait_storage::{Storage, StorageClient};

type BoxResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Clone, Debug)]
pub struct State {
    pub configs: Configs,
    pub db: MongoClient,
    pub storage: StorageClient,
    pub users_admin: UsersAdmin,
    pub last_cleanup: Arc<Mutex<i64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keys {
    pub keys: Vec<Key>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Key {
    pub ver: u8,
    pub key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretSaved {
    pub id: String,
    pub key: String,
    pub expire_seconds: i64,
    pub expire_reads: i64,
    pub pwd: bool,
    pub ignore_link_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SetResult {
    pub url: String,
    pub data: DataInfo,
    pub metadata: MetaDataInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaDataInfo {
    pub expire_seconds: i64,
    pub expire_reads: i64,
    pub pwd: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Configs {
    pub url: String,
    pub database: String,
    pub retention: i64,
    pub reads: i64,
    pub ignore_link_key: bool,
    pub encrypt_data: bool,
    pub collection_uploads: String,
    pub collection_admin: String,
    pub collection_users: String,
    pub gcs_bucket: String,
    pub keys: Keys,
}

impl Keys {
    pub fn latest_key(&self) -> Key {
        self.keys.iter().max_by_key(|x| &x.ver).unwrap().clone()
    }

    pub fn get_ver(&self, ver: u8) -> Option<&Key> {
        self.keys.iter().find(|&v| v.ver == ver)
    }
}

pub fn hash(str: &str) -> String {
    let mut hasher = Blake2s256::new();
    hasher.update(str.as_bytes());
    encode(hasher.finalize())
}

impl State {
    pub async fn new(
        opts: ArgMatches,
        mongo_client: mongodb::Client,
        storage_client: StorageClient,
    ) -> BoxResult<Self> {
        Ok(State {
            configs: Configs {
                url: opts.value_of("url").unwrap().to_string(),
                database: opts.value_of("database").unwrap().to_string(),
                retention: opts.value_of("retention").unwrap().parse()?,
                reads: opts.value_of("reads").unwrap().parse()?,
                ignore_link_key: opts.is_present("ignore_link_key"),
                encrypt_data: opts.is_present("encrypt_data"),
                gcs_bucket: opts.value_of("bucket").unwrap().to_string(),
                collection_uploads: opts.value_of("collection").unwrap().to_string(),
                collection_admin: opts.value_of("admin").unwrap().to_string(),
                collection_users: opts.value_of("users").unwrap().to_string(),
                keys: serde_json::from_str(opts.value_of("keys").unwrap())?,
            },
            users_admin: UsersAdmin::new(
                opts.value_of("database").unwrap(),
                opts.value_of("users").unwrap(),
                mongo_client.clone(),
            )
            .await?,
            db: MongoClient::new(mongo_client.clone(), opts.value_of("database").unwrap()),
            storage: storage_client,
            last_cleanup: Arc::new(Mutex::new(Utc::now().timestamp())),
        })
    }

    pub async fn increment(&self, doc_id: &str, link_id: &str) -> Result<MetaData, RestError> {
        log::debug!("Attempting to increment hit counter on {}", doc_id);
        let filter = doc! {"id": doc_id, "active": true, "links.id": link_id};
        let update = doc! { "$inc": { "lifecycle.current.reads": 1, "links.$.reads": 1 } };
        self.db
            .find_one_and_update::<MetaData>(&self.configs.collection_uploads, filter, update, None)
            .await
    }

    pub async fn delete(&self, id: &str) -> Result<(), RestError> {
        log::debug!("\"Deleting {} from mongo\"", &id);

        let filter = doc! {"id": id, "active": true};
        let update = doc! {"$set": {"active": false }};

        // Set mongo doc to active=false
        self.db
            .find_one_and_update::<Document>(&self.configs.collection_uploads, filter, update, None)
            .await?;

        // Delete object
        self.storage.delete_object(id).await?;

        Ok(())
    }

    pub async fn get(
        &mut self,
        link_id: &str,
        key: Option<&String>,
        password: Option<&String>,
    ) -> Result<(Vec<u8>, String), RestError> {
        // Kick off cleanup
        self.cleanup().await?;

        // Get doc from mongo
        let filter = doc! {"links.id": link_id, "active": true };
        let secret = self
            .db
            .find_one::<MetaData>(&self.configs.collection_uploads, filter, None)
            .await?;

        // Compare password hash
        if let Some(pwd_hash) = secret.facts.pwd {
            match password {
                Some(p) => {
                    let password_hash = hash(p);
                    if password_hash != pwd_hash {
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

        // If encryption is managed, and ignore_link_key is not false, check client key against link key
        if secret.facts.encryption.managed && !secret.facts.ignore_link_key {
            if let Some(link) = secret.links.find(link_id) {
                // This should not error
                if link.key.is_none() {
                    return Err(RestError::NotFound);
                }

                if let Some(client_key) = key {
                    let client_key_hash = hash(client_key);

                    if &client_key_hash != link.key.as_ref().unwrap() {
                        log::warn!("\"Client key did not match link key\"");
                        return Err(RestError::NotFound);
                    }
                } else {
                    return Err(RestError::NotFound);
                }
            } else {
                log::error!("Mongo returned doc that did not have matching key");
                return Err(RestError::NotFound);
            }
        }

        // If key is expired, delete
        if Utc::now().timestamp_millis() > secret.lifecycle.max.expires.timestamp_millis() {
            log::debug!("\"Key has expired: {}\"", secret.id);
            return Err(RestError::NotFound);
        }

        // Get data from storage
        let value = self.storage.fetch_object(&secret.id).await?;

        let value = if secret.facts.encryption.encrypted {
            // Get decryption key, either from the mongo doc, or from the client
            let decryption_key = if !secret.facts.encryption.managed {
                log::debug!("Using client-provided decryption key");
                match key {
                    Some(k) => k.to_owned(),
                    None => return Err(RestError::NotFound),
                }
            } else {
                let decrypt_key_ver = secret
                    .facts
                    .encryption
                    .version
                    .expect("Missing requiered encryption key version");
                let decrypt_key = self
                    .configs
                    .keys
                    .get_ver(decrypt_key_ver)
                    .expect("error getting decryption key version from mongodoc");
                let encrypted_key = secret
                    .facts
                    .encryption
                    .key
                    .expect("Missing requiered encryption key");

                // Decrypt encryption key
                let secret_key = orion::aead::SecretKey::from_slice(decrypt_key.key.as_bytes())?;
                match orion::aead::open(&secret_key, &encrypted_key) {
                    Ok(e) => {
                        let key = std::str::from_utf8(&e)?;
                        key.to_owned()
                    }
                    Err(e) => {
                        log::error!("\"Error decrypting encryption key: {}\"", e);
                        return Err(RestError::NotFound);
                    }
                }
            };

            // Decrypt data
            let secret_key = orion::aead::SecretKey::from_slice(decryption_key.as_bytes())?;
            match orion::aead::open(&secret_key, &value) {
                Ok(e) => e,
                Err(e) => {
                    log::error!("\"Error decrypting secret: {}\"", e);
                    return Err(RestError::NotFound);
                }
            }
        } else {
            value
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
            self.increment(&secret.id, link_id).await?;
        };

        Ok((value, secret.meta.content_type))
    }

    // Generate MetaData and Data from http post request, then persist in backing database and object storage
    pub async fn set(
        &mut self,
        value: Bytes,
        queries: &Query<QueriesSet>,
        headers: HeaderMap,
        current_user: CurrentUser,
    ) -> Result<SetResult, RestError> {
        // Generate MetaData doc and Data block
        let metadata_payload = MetaData::create(
            value,
            queries,
            headers,
            current_user.id,
            self.configs.clone(),
        )?;

        let results = SetResult {
            url: metadata_payload.url.clone(),
            data: DataInfo {
                id: metadata_payload.metadata.links.0[0].id.clone(),
                key: metadata_payload.key.clone(),
            },
            metadata: MetaDataInfo {
                expire_seconds: metadata_payload.metadata.lifecycle.max.seconds,
                expire_reads: metadata_payload.metadata.lifecycle.max.reads,
                pwd: queries.pwd.is_some(),
                tags: queries.tags.clone(),
            },
        };

        log::debug!(
            "\"Saving with expiration of {} seconds, and {} max expire_reads\"",
            metadata_payload.metadata.lifecycle.max.seconds,
            metadata_payload.metadata.lifecycle.max.reads
        );

        self.insert_upload(metadata_payload).await?;
        Ok(results)
    }

    pub async fn insert_upload(
        &mut self,
        metadata_payload: MetaDataPayload,
    ) -> Result<String, RestError> {
        log::debug!("here");

        // Add metadata to HashMap for object injection
        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), metadata_payload.metadata.meta.filename.clone().unwrap_or("not specified".to_owned()));
        metadata.insert("expires".to_string(), metadata_payload.metadata.lifecycle.max.expires.clone().to_string());

        log::debug!("inserting data into storage");
        self.storage
            .insert_object(
                &metadata_payload.metadata.id,
                metadata_payload.data,
                &metadata_payload.metadata.meta.content_type,
                &metadata
            )
            .await?;

        log::debug!("inserting doc into database");
        Ok(self
            .db
            .insert_one::<MetaData>(
                &self.configs.collection_uploads,
                metadata_payload.metadata,
                None,
            )
            .await?
            .links
            .first()
            .unwrap()
            .id
            .to_string())
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
                .keys(
                    doc! {"active":1, "facts.owner": 1, "lifecycle.expires_at": 1, "meta.tags": 1},
                )
                .build(),
        );

        self.db
            .create_indexes(&self.configs.collection_uploads, indexes, None)
            .await
    }

    pub async fn admin_init(&self) -> Result<(), RestError> {
        // Create cleanup lock
        let filter_lock = doc! {"name":"cleanup"};

        // Check if cleanup doc already exists, and create it if it does not
        if self
            .db
            .find_one::<Document>(&self.configs.collection_admin, filter_lock, None)
            .await
            .is_err()
        {
            log::debug!("Cleanup lock doc does not exist, creating");
            let cleanup_doc = doc! {"name":"cleanup", "active": false, "modified": Utc::now() };

            self.db
                .insert_one(&self.configs.collection_admin, cleanup_doc, None)
                .await?;
            return Ok(());
        }

        // Ensure cleanup doc is not in a "failed" state
        let filter_lock = doc! {"name":"cleanup", "active": true, "modified": { "$lt" : Utc::now() - Duration::minutes(5) } };
        let update_lock = doc! {"$set": {"active": false, "modified": Utc::now() }};
        if self
            .db
            .find_one_and_update::<Document>(
                &self.configs.collection_admin,
                filter_lock,
                update_lock,
                None,
            )
            .await
            .is_err()
        {
            log::debug!("Cleanup doc already is correct");
        };

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
        if self
            .db
            .find_one_and_update::<Document>(
                &self.configs.collection_admin,
                filter_lock,
                update_lock,
                None,
            )
            .await
            .is_err()
        {
            log::debug!("\"Cleanup not required at this time\"");
            return Err(RestError::CleanupNotRequired);
        };
        Ok(())
    }

    pub async fn unlock_cleanup(&self) -> Result<(), RestError> {
        log::debug!("\"Freeing cleanup doc\"");
        let filter_unlock = doc! {"active": true, "name":"cleanup"};
        let update_unlock = doc! {"$set": {"active": false}};

        // Unlock cleanup doc
        if self
            .db
            .find_one_and_update::<Document>(
                &self.configs.collection_admin,
                filter_unlock,
                update_unlock,
                None,
            )
            .await
            .is_err()
        {
            log::error!("Unable to free cleanup doc");
        };
        Ok(())
    }

    pub async fn expired_ids(&self) -> Result<Vec<String>, RestError> {
        // Search for docs that are expired here
        let query = doc! {"active": true, "lifecycle.max.expires": {"$lt": Utc::now()}};
        let find_options = FindOptions::builder()
            .sort(doc! { "_id": -1 })
            .limit(1000)
            .build();

        let res = self
            .db
            .find::<MetaData>(&self.configs.collection_uploads, query, Some(find_options))
            .await?;
        let result: Vec<String> = res.iter().map(|s| s.id.to_owned()).collect();
        Ok(result)
    }

    pub async fn uploads_owned(
        &self,
        id: &str,
        tags: Option<Vec<String>>,
    ) -> Result<Vec<MetaDataPublic>, RestError> {
        let query = match tags {
            Some(t) => {
                doc! {"active": true, "facts.owner": id, "lifecycle.max.expires": {"$gt": Utc::now()}, "meta.tags": { "$all": t } }
            }
            None => {
                doc! {"active": true, "facts.owner": id, "lifecycle.max.expires": {"$gt": Utc::now()}}
            }
        };

        let find_options = FindOptions::builder()
            .sort(doc! { "_id": -1 })
            .limit(1000)
            .build();

        let res = self
            .db
            .find::<MetaData>(&self.configs.collection_uploads, query, Some(find_options))
            .await?;
        let result: Vec<MetaDataPublic> = res.iter().map(|s| s.to_json()).collect();
        Ok(result)
    }

    pub async fn get_doc(&self, user_id: &str, doc_id: &str) -> Result<MetaDataPublic, RestError> {
        let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
        Ok(self
            .db
            .find_one::<MetaData>(&self.configs.collection_uploads, filter, None)
            .await?
            .to_json())
    }

    pub async fn delete_doc(&self, user_id: &str, doc_id: &str) -> Result<(), RestError> {
        let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
        // Ensure that doc exists, and is owned by user
        self.db
            .find_one::<MetaData>(&self.configs.collection_uploads, filter, None)
            .await?;
        self.delete(doc_id).await?;
        Ok(())
    }

    pub async fn delete_link(
        &self,
        user_id: &str,
        doc_id: &str,
        link_id: &str,
    ) -> Result<(), RestError> {
        let filter =
            doc! {"active": true, "facts.owner": user_id, "id": doc_id, "links.id": link_id };
        let update = doc! { "$pull": { "links": { "id": link_id } } };
        // Ensure that doc exists, and is owned by user
        self.db
            .find_one_and_update::<MetaData>(&self.configs.collection_uploads, filter, update, None)
            .await?;
        Ok(())
    }

    pub async fn get_links(
        &self,
        user_id: &str,
        doc_id: &str,
    ) -> Result<Vec<LinkScrubbed>, RestError> {
        log::debug!("Attempting to locate doc: {}", doc_id);
        let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
        Ok(self
            .db
            .find_one::<MetaData>(&self.configs.collection_uploads, filter, None)
            .await?
            .links
            .to_vec())
    }

    pub async fn add_link(
        &self,
        user_id: &str,
        doc_id: &str,
        tags: Option<Vec<String>>,
    ) -> Result<(LinkWithKey, Option<String>, bool), RestError> {
        log::debug!("Attempting to locate doc to add link: {}", doc_id);
        let new_link = Link::new(Some(&user_id.to_owned()), tags)?;
        let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
        let update = doc! { "$push": { "links": to_document(&new_link.link)? } };
        let doc = self
            .db
            .find_one_and_update::<MetaData>(&self.configs.collection_uploads, filter, update, None)
            .await?;
        Ok((
            new_link,
            doc.meta.filename.clone(),
            doc.facts.ignore_link_key,
        ))
    }

    pub async fn add_doc_tags(
        &self,
        user_id: &str,
        doc_id: &str,
        tags: Option<Vec<String>>,
    ) -> Result<Vec<String>, RestError> {
        if let Some(tags_unwrapped) = tags {
            log::debug!("Attempting to locate doc to add tags: {}", doc_id);
            let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
            let update = doc! { "$addToSet": { "meta.tags": { "$each": tags_unwrapped.clone() } } };
            self.db
                .find_one_and_update::<MetaData>(
                    &self.configs.collection_uploads,
                    filter,
                    update,
                    None,
                )
                .await?;
            Ok(tags_unwrapped)
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn delete_doc_tags(
        &self,
        user_id: &str,
        doc_id: &str,
        tags: Option<Vec<String>>,
    ) -> Result<Vec<String>, RestError> {
        if let Some(tags_unwrapped) = tags {
            log::debug!("Attempting to locate doc to delete tags: {}", doc_id);
            let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
            let update = doc! { "$pull": { "meta.tags": { "$in": tags_unwrapped.clone() } } };
            self.db
                .find_one_and_update::<MetaData>(
                    &self.configs.collection_uploads,
                    filter,
                    update,
                    None,
                )
                .await?;
            Ok(tags_unwrapped)
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_doc_tags(
        &self,
        user_id: &str,
        doc_id: &str,
    ) -> Result<Vec<String>, RestError> {
        log::debug!("Attempting to locate doc to get tags: {}", doc_id);
        let filter = doc! {"active": true, "facts.owner": user_id, "id": doc_id };
        let doc = self
            .db
            .find_one::<MetaData>(&self.configs.collection_uploads, filter, None)
            .await?;
        if let Some(tags) = doc.meta.tags {
            Ok(tags)
        } else {
            Ok(Vec::new())
        }
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
        // Send initialization to background thread
        let mut me = self.clone();
        tokio::spawn(async move {
            if me.admin_init().await.is_err() {
                log::error!("Error initializing admin collection");
            };
            if me.cleanup_init().await.is_err() {
                log::error!("Error starting initial cleanup");
            };
            if me.create_uploads_indexes().await.is_err() {
                log::error!("Error creating upload collection indexes");
            };
        });
        Ok(())
    }

    //
    // Send User Requests
    //

    pub async fn create_user(&self, email: &str, pwd: &str) -> Result<String, RestError> {
        self.users_admin.create_user(email, pwd).await
    }

    pub async fn get_user_id(&self, email: &str, pwd: &str) -> Result<String, RestError> {
        self.users_admin.get_user_id(email, pwd).await
    }

    pub async fn create_api_key(
        &self,
        id: &str,
        tags: Option<Vec<String>>,
        role: Option<String>,
    ) -> Result<ApiKey, RestError> {
        self.users_admin.create_api_key(id, tags, role).await
    }

    pub async fn list_api_keys(&self, id: &str) -> Result<Vec<ApiKeyBrief>, RestError> {
        match self.users_admin.list_api_keys(id).await {
            Ok(u) => Ok(u),
            Err(e) => Err(e),
        }
    }

    pub async fn delete_api_key(&self, id: &str, key: &str) -> Result<bool, RestError> {
        self.users_admin.delete_api_key(id, key).await
    }
}
