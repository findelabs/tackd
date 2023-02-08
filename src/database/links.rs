use blake2::{Blake2s256, Digest};
use chrono::Utc;
use hex::encode;
use rand::distributions::Alphanumeric;
use rand::distributions::DistString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

use crate::error::Error as RestError;
use crate::state::Configs;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Links(pub Vec<Link>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinksScrubbed(pub Vec<LinkScrubbed>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub key: Option<String>, // Hashed decryption key
    pub created: chrono::DateTime<Utc>,
    pub reads: i64,
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewLinkResult {
    pub filename: Option<String>,
    pub link_with_key: LinkWithKey
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewLinkResultJson {
    pub url: String,
    pub data: LinkSecret
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkSecret {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>, // Decryption key
    pub created: chrono::DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkScrubbed {
    pub id: String,
    pub created: chrono::DateTime<Utc>,
    pub reads: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkWithKey {
    pub link: Link,
    pub url: String,
    pub key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct KeyPair {
    key_raw: String,
    key_hashed: String,
}

impl KeyPair {
    fn new() -> Self {
        // Generate unlock key
        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

        // Hash unlock key
        let mut hasher = Blake2s256::new();
        hasher.update(key.as_bytes());
        let hashed = encode(hasher.finalize());

        KeyPair {
            key_raw: key,
            key_hashed: hashed,
        }
    }
}

impl LinkWithKey {
    pub fn to_json(&self) -> LinkSecret {
        LinkSecret {
            id: self.link.id.clone(),
            key: self.key.clone(),
            created: self.link.created,
            tags: self.link.tags.clone(),
        }
    }
}

impl Links {
    pub fn find(&self, id: &str) -> Option<&Link> {
        self.0.iter().find(|&v| v.id == id)
    }

    pub fn first(&self) -> Option<&Link> {
        self.0.first()
    }

    //    pub fn last(&self) -> Option<&Link> {
    //        self.0.last()
    //    }

    pub fn to_vec(&self) -> Vec<LinkScrubbed> {
        self.0.iter().map(|s| s.scrub()).collect()
    }
}

impl NewLinkResult {
    pub fn to_json(&self) -> NewLinkResultJson {

        // If filename was passed, include filename is in the path
        let mut query_map = HashMap::new();
        let file = if let Some(filename) = &self.filename {
            query_map.insert("id", self.link_with_key.link.id.clone());
            filename.to_owned()
        } else {
            self.link_with_key.link.id.clone()
        };

        if let Some(key) = self.link_with_key.key.as_ref() {
            query_map.insert("key", key.clone());
        };

        let spacer = if query_map.len() > 0 { "?" } else { "" };

        let url = format!(
            "{}/download/{}{}{}",
            self.link_with_key.url,
            file,
            spacer,
            serde_urlencoded::to_string(query_map).expect("Could not parse query hashmap")
        );

        NewLinkResultJson {
            url,
            data: self.link_with_key.to_json()
        }
    }
}

impl Link {
    pub fn default(configs: &Configs) -> LinkWithKey {
        LinkWithKey {
            link: Link {
                id: Uuid::new_v4().to_string(),
                key: None,
                created: Utc::now(),
                reads: 0,
                tags: None,
            },
            key: None,
            url: configs.url.clone()
        }
    }

    pub fn scrub(&self) -> LinkScrubbed {
        LinkScrubbed {
            id: self.id.clone(),
            created: self.created,
            reads: self.reads,
            tags: self.tags.clone(),
        }
    }

    pub fn new(
        current_user: Option<&String>,
        configs: &Configs,
        tags: Option<Vec<String>>,
    ) -> Result<LinkWithKey, RestError> {
        // Is this is an unknown user, return "default"
        if current_user.is_none() {
            log::debug!("Generating default link");
            return Ok(Self::default(configs));
        };

        let (key_raw, key_hashed) = match configs.ignore_link_key {
            true => (None, None),
            false => {
                let key_pair = KeyPair::new();
                (Some(key_pair.key_raw), Some(key_pair.key_hashed))
            }
        };

        Ok(LinkWithKey {
            key: key_raw,
            url: configs.url.clone(),
            link: Link {
                id: Uuid::new_v4().to_string(),
                key: key_hashed,
                created: Utc::now(),
                reads: 0,
                tags
            }
        })
    }
}
