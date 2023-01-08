use blake2::{Blake2s256, Digest};
use chrono::Utc;
use hex::encode;
use rand::distributions::Alphanumeric;
use rand::distributions::DistString;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error as RestError;

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
pub struct LinkSecret {
    pub id: String,
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

impl Link {
    pub fn default() -> LinkWithKey {
        LinkWithKey {
            link: Link {
                id: Uuid::new_v4().to_string(),
                key: None,
                created: Utc::now(),
                reads: 0,
                tags: None,
            },
            key: None,
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

    // Return tuple of (decryption key, Link)
    pub fn new(
        current_user: Option<&String>,
        tags: Option<Vec<String>>,
    ) -> Result<LinkWithKey, RestError> {
        // Is this is an unknown user, return "default"
        if current_user.is_none() {
            return Ok(Self::default());
        };

        let key_pair = KeyPair::new();

        Ok(LinkWithKey {
            key: Some(key_pair.key_raw),
            link: Link {
                id: Uuid::new_v4().to_string(),
                key: Some(key_pair.key_hashed),
                created: Utc::now(),
                reads: 0,
                tags,
            },
        })
    }
}
