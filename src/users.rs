use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{Utc, DateTime};
use mongodb::Collection;
use bson::{doc, to_document, from_document, Document};
use blake2::{Blake2s256, Blake2b, Digest, digest::consts::U10};
use hex::encode;
use mongodb::IndexModel;
use mongodb::options::IndexOptions;

use crate::error::Error as RestError;

#[derive(Clone, Debug)]
pub struct UsersAdmin {
    pub database: String,
    pub collection: String,
    pub mongo_client: mongodb::Client
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub email: String,
    pub id: String,
    pub pwd: String,
    pub created: DateTime<Utc>,
    pub api_keys: Vec<ApiKeyHashed>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKey {
    pub key: String,
    pub secret: String,
    pub created: DateTime<Utc>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKeyBrief {
    pub key: String,
    pub created: DateTime<Utc>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKeyHashed {
    pub key: String,
    pub secret: String,
    pub created: DateTime<Utc>
}

impl ApiKey {
    pub fn new() -> ApiKey {
        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);
        let uuid = Uuid::new_v4().to_string();
        let mut hasher = Blake2b::<U10>::new();
        hasher.update(uuid);
        let secret = encode(hasher.finalize());

        ApiKey { key, secret, created: Utc::now() }
    }
    
    pub fn hashed(&self) -> ApiKeyHashed {
        ApiKeyHashed {
            key: self.key.clone(),
            secret: User::hash(&self.secret),
            created: self.created
        }
    }
}

impl User {
    pub fn hash(email: &str) -> String {
        let mut hasher = Blake2s256::new();
        hasher.update(email.as_bytes());
        encode(hasher.finalize().to_vec())
    }

    pub fn new(email: &str, pwd: &str) -> User {
        let email = User::hash(email);
        let pwd = User::hash(pwd);
        let id = Uuid::new_v4().to_string();

        User { email, pwd, id, api_keys: Vec::new(), created: Utc::now() }
    }
}

impl UsersAdmin {
    pub async fn new(db: &str, coll: &str, mongo_client: mongodb::Client) -> Result<UsersAdmin, RestError> {
        let mut users_admin = UsersAdmin {
            database: db.to_owned(),
            collection: coll.to_owned(),
            mongo_client
        };
        users_admin.create_indexes().await?;
        Ok(users_admin)
    }

    pub fn collection(&self) -> Collection<Document> {
        self.mongo_client
            .database(&self.database)
            .collection(&self.collection)
    }

    pub async fn get_user(&self, email: &str) -> Result<User, RestError> {
        let filter = doc! {"email": &User::hash(email) };
        match self.collection().find_one(Some(filter), None).await {
            Ok(v) => match v {
                Some(v) => Ok(from_document(v)?),
                None => Err(RestError::NotFound),
            },
            Err(e) => {
                log::error!("Error getting user {}: {}", email, e);
                Err(RestError::NotFound)
            }
        }
    }

//    pub async fn validate_user(&self, id: &str, pwd: &str) -> Result<User, RestError> {
//        let filter = doc! {"id": id, "pwd": User::hash(pwd) };
//        match self.collection().find_one(Some(filter), None).await {
//            Ok(v) => match v {
//                Some(v) => Ok(from_document(v)?),
//                None => Err(RestError::BadLogin),
//            },
//            Err(e) => {
//                log::error!("Error getting user {}: {}", id, e);
//                Err(RestError::NotFound)
//            }
//        }
//    }

    pub async fn create_user(&self, email: &str, password: &str) -> Result<String, RestError> {
        if self.get_user(email).await.is_ok() {
            return Err(RestError::UserExists)
        }

        let user = User::new(email, password);
        let user_doc = to_document(&user)?;

        match self.collection().insert_one(user_doc, None).await {
            Ok(_) => Ok(user.id),
            Err(e) => {
                log::error!("Error creating new user {}: {}", email, e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn create_api_key(&self, id: &str) -> Result<ApiKey, RestError> {
        let api_key = ApiKey::new();

        let filter = doc! {"id": &id };
        let update = doc! {"$push": {"api_keys": to_document(&api_key.hashed())? }};

        match self.collection().find_one_and_update(filter, update, None).await {
            Ok(_) => Ok(api_key),
            Err(e) => {
                log::error!("Error creating new api key {}: {}", id, e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn delete_api_key(&self, id: &str, key: &str) -> Result<bool, RestError> {
        log::debug!("\"Trying to delete {} from {}", key, id);
        let filter = doc! {"id": &id, "api_keys.key": key };
        let update = doc! {"$pull": {"api_keys": { "key": key } }};

        match self.collection().find_one_and_update(filter, update, None).await {
            Ok(m) => match m {
                Some(_) => Ok(true),
                None => Ok(false)
            },
            Err(e) => {
                log::error!("Error creating new api key {}: {}", id, e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn list_api_keys(&self, id: &str) -> Result<Vec<ApiKeyBrief>, RestError> {
        let filter = doc! {"id": &id };
        let coll = self.mongo_client.database(&self.database).collection::<User>(&self.collection);

        match coll.find_one(Some(filter), None).await {
            Ok(v) => match v {
                Some(u) => {
                    Ok(u.api_keys.iter().map(|s| ApiKeyBrief { key: s.key.to_owned(), created: s.created.to_owned() }).collect())
                },
                None => Err(RestError::BadLogin),
            },
            Err(e) => {
                log::error!("Error getting user {}: {}", id, e);
                Err(RestError::NotFound)
            }
        }
    }

//    pub async fn validate_api_key(&self, key: &str, secret: &str) -> Result<String, RestError> {
//        let filter = doc! {"api_keys.key": key, "api_keys.secret": User::hash(secret) };
//        let coll = self.mongo_client.database(&self.database).collection::<User>(&self.collection);
//        match coll.find_one(Some(filter), None).await {
//            Ok(v) => match v {
//                Some(u) => Ok(u.id),
//                None => Err(RestError::BadLogin),
//            },
//            Err(e) => {
//                log::error!("Error getting user {}: {}", key, e);
//                Err(RestError::NotFound)
//            }
//        }
//    }

    pub async fn validate_user_or_api_key(&self, id: &str, pwd: &str) -> Result<String, RestError> {
        let filter = doc! {"$or": [ {"id": id, "pwd": User::hash(pwd) }, { "api_keys.key": id, "api_keys.secret": User::hash(pwd) } ] };
        let coll = self.mongo_client.database(&self.database).collection::<User>(&self.collection);
        match coll.find_one(Some(filter), None).await {
            Ok(v) => match v {
                Some(v) => Ok(v.id),
                None => Err(RestError::BadLogin),
            },
            Err(e) => {
                log::error!("Error finding user or api key {}: {}", id, e);
                Err(RestError::NotFound)
            }
        }
    }
    pub async fn create_indexes(&mut self) -> Result<(), RestError> {
        log::debug!("Creating users collection indexes");
        let mut indexes = Vec::new();
        indexes.push(
            IndexModel::builder()
                .keys(doc! {"api_keys.key":1, "api_keys.secret": 1})
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        );

        indexes.push(
            IndexModel::builder()
                .keys(doc! {"id":1, "pwd": 1})
                .options(IndexOptions::builder().unique(true).build())
                .build(),
        );

        indexes.push(
            IndexModel::builder()
                .keys(doc! {"email":1})
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
}
