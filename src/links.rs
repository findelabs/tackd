use serde::{Deserialize, Serialize};
use uuid::Uuid;
use blake2::{Digest, Blake2s256};
use hex::encode;
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::distributions::DistString;

use crate::error::Error as RestError;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Links(pub Vec<Link>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub key: Option<String>,        // Hashed decryption key
    pub created: chrono::DateTime<Utc>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkScrubbed {
    pub id: String,
    pub key: Option<String>,        // Decryption key
    pub created: chrono::DateTime<Utc>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkWithKey {
    pub link: Link,
    pub key: Option<String>
}

#[derive(Clone, Debug)]
pub struct KeyPair {
    key_raw: String,
    key_hashed: String
}

impl KeyPair {
    fn new() -> Self {
        // Generate unlock key
        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 32);

        // Hash unlock key
        let mut hasher = Blake2s256::new();
        hasher.update(key.as_bytes());
        let hashed = encode(hasher.finalize().to_vec());

        KeyPair {
            key_raw: key,
            key_hashed: hashed
        }
    }
}

impl LinkWithKey {
    pub fn to_json(&self) -> LinkScrubbed {
        LinkScrubbed {
            id: self.link.id.clone(),
            key: self.key.clone(),
            created: self.link.created
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
}

impl Link {
    pub fn default() -> LinkWithKey {
        LinkWithKey {
            link: Link {
                id: Uuid::new_v4().to_string(),
                key: None,
                created: Utc::now()
            },
            key: None
        }
    }

    // Return tuple of (decryption key, Link)
    pub fn new(current_user: Option<&String>) -> Result<LinkWithKey, RestError> {

        // Is this is an unknown user, return "default"
        if current_user.is_none() {
            return Ok(Self::default())
        };

        let key_pair = KeyPair::new();

        Ok(LinkWithKey {
            key: Some(key_pair.key_raw),
            link: Link {
                id: Uuid::new_v4().to_string(),
                key: Some(key_pair.key_hashed),
                created: Utc::now()
            }
        })
    }
}
