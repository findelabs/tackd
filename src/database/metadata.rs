use serde::{Deserialize, Serialize};
use chrono::{Duration, Utc};
use uuid::Uuid;
use hyper::HeaderMap;
use hyper::header::{CONTENT_TYPE, USER_AGENT};
use hex::encode;
use blake2::{Blake2s256, Digest};
use ms_converter::ms;
use axum::body::Bytes;
use axum::extract::Query;
use std::convert::From;
use std::collections::HashMap;

use crate::database::links::{Link, LinkScrubbed, Links};
use crate::state::Configs;
use crate::data::Data;
use crate::error::Error as RestError;
use crate::handlers::QueriesSet;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaData {
    pub id: String,
    pub active: bool,
    pub meta: Meta,
    pub lifecycle: Lifecycle,
    pub facts: Facts,
    pub links: Links,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaDataPublic {
    pub id: String,
    pub meta: Meta,
    pub lifecycle: LifecyclePublic,
    pub links: Vec<LinkScrubbed>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    pub created: chrono::DateTime<Utc>,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x_forwarded_for: Option<String>,
    pub bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lifecycle {
    pub max: LifecycleMax,
    pub current: LifecycleCurrent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecyclePublic {
    pub max: LifecycleMaxJson,
    pub current: LifecycleCurrent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleMax {
    pub reads: i64,
    pub seconds: i64,
    pub expires: bson::DateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleMaxJson {
    pub reads: i64,
    pub seconds: i64,
    pub expires: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LifecycleCurrent {
    pub reads: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Encryption {
    pub managed: bool,
    #[serde(with = "serde_bytes")]
    pub key: Option<Vec<u8>>,
    pub version: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Facts {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    // recipients: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pwd: Option<String>,
    pub encryption: Encryption,
    pub ignore_link_key: bool,
}

#[derive(Clone, Debug)]
pub struct MetaDataPayload {
    pub metadata: MetaData,
    // If data is not encrypted, then this will be None
    pub key: Option<String>,
    pub data: Vec<u8>,
    pub url: String
}

impl From<LifecycleMax> for LifecycleMaxJson {
    fn from(item: LifecycleMax) -> Self {
        // Try to convert expiration to human-readable string, or revert to seconds
        let expires = match item.expires.try_to_rfc3339_string() {
            Ok(t) => t,
            Err(_) => (item.expires.timestamp_millis() / 1000).to_string()
        };

        LifecycleMaxJson {
            reads: item.reads.clone(),
            seconds: item.seconds.clone(),
            expires: expires
        }
    }
}

impl From<Lifecycle> for LifecyclePublic {
    fn from(item: Lifecycle) -> Self {
        LifecyclePublic {
            max: item.max.into(),
            current: item.current
        }
    }
}

impl MetaData {
    pub fn to_json(&self) -> MetaDataPublic {
        MetaDataPublic {
            id: self.id.clone(),
            meta: self.meta.clone(),
            lifecycle: self.lifecycle.clone().into(),
            links: self.links.to_vec(),
        }
    }

    pub fn create(
        payload: Bytes,
        queries: &Query<QueriesSet>,
        headers: HeaderMap,
        current_user: Option<String>,
        configs: Configs
    ) -> Result<MetaDataPayload, RestError> {
        let id = Uuid::new_v4().to_string();
        log::debug!("Sealing up data as object {}", &id);

        // Generate Data from payload
        // None (second param) is for when we allow users to specify their own encryption key
        let data = Data::create(payload, None, &configs.keys, current_user.is_some(), configs.encrypt_data)?;

        // Create initial link to brand new document
        let link = Link::new(current_user.as_ref(), None)?;

        // If user is unknown, we will only be generating a single link for this doc,
        // so use the dencryption key
        let initial_url_key = link.key.or(data.key);

        // If neither expiration reads nor seconds is specified, then read expiration should default to one
        let expire_reads = if let Some(expire_reads) = queries.reads {
            expire_reads
        } else if queries.expires.is_none() {
            configs.reads
        } else {
            -1
        };

        // Ensure max expire_seconds is less than a month
        let expire_seconds = match &queries.expires {
            Some(expires) => match expires.parse::<i64>() {
                Ok(seconds) => seconds,
                Err(_) => {
                    let s = ms(expires)? / 1000;
                    if s > 220752000i64 {
                        log::warn!(
                            "Incorrect expiration seconds requested, defaulting to seven years"
                        );
                        220752000i64
                    } else {
                        s
                    }
                }
            },
            None => {
                log::debug!("No expiration set, defaulting to {} seconds", configs.retention);
                configs.retention
            }
        };

        // Hash password if one was provided
        let pwd = match &queries.pwd {
            Some(p) => {
                let mut hasher = Blake2s256::new();
                hasher.update(p.as_bytes());
                Some(encode(hasher.finalize()))
            }
            None => None,
        };

        // Detect binary mime-type, fallback on content-type header
        let content_type = match data.mime_type {
            Some(m) => m,
            None => match headers.get(CONTENT_TYPE) {
                Some(h) => h.to_str().unwrap_or("none").to_owned(),
                None => "none".to_owned(),
            }
        };

        // If filename was passed, include filename is in the path
        let mut query_map = HashMap::new();
        let file = if let Some(filename) = &queries.filename {
            query_map.insert("id", id.clone());
            filename.to_owned()
        } else {
            id.clone()
        };
        
        if let Some(key) = initial_url_key.as_ref() {
            if current_user.is_none() && !configs.ignore_link_key {
                query_map.insert("key", key.clone());
            };
        };

        let spacer = if query_map.len() > 0 {
            "&"
        } else {
            ""
        };
            
        let metadata = MetaData {
            id,
            active: true,
            meta: Meta {
                created: Utc::now(),
                content_type,
                expires: queries.expires.clone(),
                bytes: data.data.len(),
                x_forwarded_for: headers.get("x-forwarded-for").map(|s| s.to_str().unwrap_or("error").to_string()),
                user_agent: headers.get(USER_AGENT).map(|s| s.to_str().unwrap_or("error").to_string()),
                filename: queries.filename.clone(),
                tags: queries.tags.clone(),
            },
            lifecycle: Lifecycle {
                max: LifecycleMax {
                    reads: expire_reads,
                    seconds: expire_seconds,
                    expires: (Utc::now() + Duration::seconds(expire_seconds)).into(), // Secret expiration is now + expiration seconds
                },
                current: LifecycleCurrent { reads: 0i64 },
            },
            facts: Facts {
                owner: current_user,
                // recipients, # Future capability
                pwd,
                encryption: Encryption { 
                    managed: data.encrypted_key.is_some(), 
                    key: data.encrypted_key, 
                    version: data.encrypted_key_version 
                },
                ignore_link_key: configs.ignore_link_key,
            },
            links: Links(vec![link.link]),
        };

        let url = format!("{}/download/{}{}{}", configs.url, file, spacer, serde_urlencoded::to_string(query_map).expect("Could not parse query hashmap"));

        Ok(MetaDataPayload {
            metadata,
            key: initial_url_key,
            data: data.data,
            url
        })
    }
}
