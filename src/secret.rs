use axum::body::Bytes;
use axum::extract::Query;
use blake2::{Blake2s256, Digest};
use chrono::{Duration, Utc};
use hex::encode;
use hyper::header::{CONTENT_TYPE, USER_AGENT};
use hyper::HeaderMap;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error as RestError;
use crate::handlers::QueriesSet;
use crate::links::{Link, LinkScrubbed, Links};
use crate::state::Keys;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Secret {
    pub id: String,
    pub active: bool,
    pub meta: Meta,
    pub lifecycle: Lifecycle,
    pub facts: Facts,
    pub links: Links,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecretScrubbed {
    pub id: String,
    pub meta: Meta,
    pub lifecycle: LifecycleScrubbed,
    pub links: Vec<LinkScrubbed>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_forwarded_for: Option<String>,
    pub bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lifecycle {
    pub max: LifecycleMax,
    pub current: LifecycleCurrent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleScrubbed {
    pub max: LifecycleMaxScrubbed,
    pub current: LifecycleCurrent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleMax {
    pub reads: i64,
    pub seconds: i64,
    pub expires: bson::DateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleMaxScrubbed {
    pub reads: i64,
    pub seconds: i64,
    pub expires: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleCurrent {
    pub reads: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    //    recipients: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pwd: Option<String>,
    pub encryption: Encryption,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Encryption {
    pub managed: bool,
    #[serde(with = "serde_bytes")]
    pub key: Option<Vec<u8>>,
    pub version: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct SecretPlusData {
    pub secret: Secret,
    pub key: String,
    pub value: Vec<u8>,
}

impl Encryption {
    pub fn new(
        current_user: &Option<String>,
        keys: &Keys,
        key: String,
    ) -> Result<Encryption, RestError> {
        // Is this is an unknown user, return "default"
        if current_user.is_none() {
            return Ok(Encryption {
                managed: false,
                key: None,
                version: None,
            });
        };

        // If user is known, encrypt encryption key for storage
        let latest_encrypt_key = keys.latest_key();
        let (_, key_encrypted) = Secret::seal(Some(&latest_encrypt_key.key), Bytes::from(key))?;

        Ok(Encryption {
            managed: true,
            key: Some(key_encrypted),
            version: Some(latest_encrypt_key.ver),
        })
    }
}

impl Secret {
    pub fn to_json(&self) -> SecretScrubbed {
        SecretScrubbed {
            id: self.id.clone(),
            meta: self.meta.clone(),
            lifecycle: LifecycleScrubbed {
                current: self.lifecycle.current.clone(),
                max: LifecycleMaxScrubbed {
                    reads: self.lifecycle.max.reads,
                    seconds: self.lifecycle.max.seconds,
                    expires: self.lifecycle.max.expires.timestamp_millis() / 1000,
                },
            },
            links: self.links.to_vec(),
        }
    }

    pub fn seal(key: Option<&str>, data: Bytes) -> Result<(String, Vec<u8>), RestError> {
        // Generate random encryption key is None is passed
        let key = match key {
            Some(k) => k.to_owned(),
            None => Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
        };

        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;

        // Encrypt data with key
        let ciphertext = match orion::aead::seal(&secret_key, &data) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e));
            }
        };
        Ok((key, ciphertext))
    }

    pub fn create(
        value: Bytes,
        queries: &Query<QueriesSet>,
        headers: HeaderMap,
        current_user: Option<String>,
        keys: &Keys,
    ) -> Result<SecretPlusData, RestError> {
        let id = Uuid::new_v4().to_string();
        log::debug!("Sealing up data as {}", &id);

        // Detect binary mime-type, fallback on content-type header
        let content_type = match infer::get(&value) {
            Some(t) => {
                let mime_type = t.mime_type().to_owned();
                log::debug!("\"Detected mime type as {}\"", &mime_type);
                mime_type
            }
            None => match headers.get(CONTENT_TYPE) {
                Some(h) => h.to_str().unwrap_or("error").to_owned(),
                None => "none".to_owned(),
            },
        };

        // Encrypt data and get back the key
        let (key, ciphertext) = Secret::seal(None, value)?;

        // Generate encryption block for doc
        let encryption_block = Encryption::new(&current_user, keys, key.clone())?;

        // Create first link to new doc
        let link_with_key = Link::new(current_user.as_ref())?;

        // If link has no key, use client provided key

        let initial_url_key = match link_with_key.key {
            Some(k) => k,
            None => key,
        };

        // If neither expiration reads nor seconds is specified, then read expiration should default to one
        let expire_reads = if let Some(expire_reads) = queries.reads {
            expire_reads
        } else if queries.expires.is_none() {
            1
        } else {
            -1
        };

        // Ensure max expire_seconds is less than a month
        let expire_seconds = match queries.expires {
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
        let pwd = match &queries.pwd {
            Some(p) => {
                let mut hasher = Blake2s256::new();
                hasher.update(p.as_bytes());
                Some(encode(hasher.finalize().to_vec()))
            }
            None => None,
        };

        // Get x-forwarded-for header
        let x_forwarded_for = headers
            .get("x-forwarded-for")
            .map(|s| s.to_str().unwrap_or("error").to_string());

        // Get user-agent header
        let user_agent = headers
            .get(USER_AGENT)
            .map(|s| s.to_str().unwrap_or("error").to_string());

        let secret = Secret {
            id,
            active: true,
            meta: Meta {
                content_type,
                bytes: ciphertext.len(),
                x_forwarded_for,
                user_agent,
                filename: queries.filename.clone(),
            },
            lifecycle: Lifecycle {
                max: LifecycleMax {
                    reads: expire_reads,
                    seconds: expire_seconds,
                    expires: expires_at.into(),
                },
                current: LifecycleCurrent { reads: 0i64 },
            },
            facts: Facts {
                owner: current_user,
                // recipients,
                pwd,
                encryption: encryption_block,
            },
            links: Links(vec![link_with_key.link]),
        };

        Ok(SecretPlusData {
            secret,
            key: initial_url_key,
            value: ciphertext,
        })
    }
}
