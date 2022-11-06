use axum::body::Bytes;
use chrono::{Duration, Utc};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

use crate::error::Error as RestError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Secret {
    pub id: String,
    pub active: bool,
    pub meta: Meta,
    pub lifecycle: Lifecycle,
    pub facts: Facts,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub content_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lifecycle {
    pub max: LifecycleMax,
    pub expires_at: bson::DateTime,
    pub reads: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleMax {
    pub reads: i64,
    pub seconds: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facts {
    //    owner: String,
    //    recipients: Vec<String>,
    pub password: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct SecretPlusData {
    pub secret: Secret,
    pub key: String,
    pub value: Vec<u8>,
}

impl Secret {
    pub fn create(
        value: Bytes,
        expire_reads: Option<i64>,
        expire_seconds: Option<i64>,
        content_type: String,
        password: Option<&String>,
    ) -> Result<SecretPlusData, RestError> {
        let id = Uuid::new_v4().to_string();
        log::debug!("Sealing up data as {}", &id);

        // Generate random encryption key
        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);
        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;

        // Encrypt data with key
        let ciphertext = match orion::aead::seal(&secret_key, &value) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e));
            }
        };

        // If neither expiration reads nor seconds is specified, then read expiration should default to one
        let expire_reads = if let Some(expire_reads) = expire_reads {
            expire_reads
        } else if expire_seconds.is_none() {
            1
        } else {
            -1
        };

        // Ensure max expire_seconds is less than a month
        let expire_seconds = match expire_seconds {
            Some(v) => {
                if v > 2592000i64 {
                    log::warn!("Incorrect expire_seconds requested, defaulting to 2,592,000");
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

        // Secret expiration is now + expiration seconds
        let expires_at = Utc::now() + Duration::seconds(expire_seconds);

        // Hash password if one was provided
        let password = match password {
            Some(p) => {
                let mut hasher = DefaultHasher::new();
                p.hash(&mut hasher);
                Some(hasher.finish() as i64)
            }
            None => None,
        };

        let secret = Secret {
            id,
            active: true,
            meta: Meta { content_type },
            lifecycle: Lifecycle {
                max: LifecycleMax {
                    reads: expire_reads,
                    seconds: expire_seconds,
                },
                expires_at: expires_at.into(),
                reads: 0i64,
            },
            facts: Facts {
                //                submitter,
                //                recipients,
                password,
            },
        };

        Ok(SecretPlusData {
            secret,
            key,
            value: ciphertext,
        })
    }
}
