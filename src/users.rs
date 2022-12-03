use blake2::{digest::consts::U10, Blake2b, Blake2s256, Digest};
use bson::{doc, to_document};
use chrono::{DateTime, Utc};
use hex::encode;
use mongodb::options::IndexOptions;
use mongodb::IndexModel;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error as RestError;
use crate::mongo::MongoClient;

#[derive(Clone, Debug)]
pub struct UsersAdmin {
    pub database: String,
    pub collection: String,
    pub db: MongoClient,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub email: String,
    pub id: String,
    pub pwd: String,
    pub created: DateTime<Utc>,
    pub api_keys: Vec<ApiKeyHashed>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKey {
    pub key: String,
    pub secret: String,
    pub created: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKeyBrief {
    pub key: String,
    pub created: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiKeyHashed {
    pub key: String,
    pub secret: String,
    pub created: DateTime<Utc>,
    pub tags: Option<Vec<String>>,
}

impl ApiKey {
    pub fn new(tags: Option<Vec<String>>) -> ApiKey {
        let key = Alphanumeric.sample_string(&mut rand::thread_rng(), 8);
        let uuid = Uuid::new_v4().to_string();
        let mut hasher = Blake2b::<U10>::new();
        hasher.update(uuid);
        let secret = encode(hasher.finalize());

        ApiKey {
            key,
            secret,
            created: Utc::now(),
            tags,
        }
    }

    pub fn hashed(&self) -> ApiKeyHashed {
        ApiKeyHashed {
            key: self.key.clone(),
            secret: User::hash(&self.secret),
            created: self.created,
            tags: self.tags.clone(),
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

        User {
            email,
            pwd,
            id,
            api_keys: Vec::new(),
            created: Utc::now(),
        }
    }
}

impl UsersAdmin {
    pub async fn new(
        db: &str,
        coll: &str,
        mongo_client: mongodb::Client,
    ) -> Result<UsersAdmin, RestError> {
        let mut users_admin = UsersAdmin {
            database: db.to_owned(),
            collection: coll.to_owned(),
            db: MongoClient::new(mongo_client.clone(), db),
        };
        users_admin.create_indexes().await?;
        Ok(users_admin)
    }

    pub async fn get_user(&self, email: &str) -> Result<User, RestError> {
        let filter = doc! {"email": &User::hash(email) };
        self.db
            .find_one::<User>(&self.collection, filter, None)
            .await
    }

    pub async fn validate_email(&self, email: &str, pwd: &str) -> Result<User, RestError> {
        let filter = doc! {"email": User::hash(email), "pwd": User::hash(pwd) };
        match self
            .db
            .find_one::<User>(&self.collection, filter, None)
            .await
        {
            Ok(v) => Ok(v),
            Err(_) => Err(RestError::BadLogin),
        }
    }

    pub async fn create_user(&self, email: &str, password: &str) -> Result<String, RestError> {
        if self.get_user(email).await.is_ok() {
            return Err(RestError::UserExists);
        }
        Ok(self
            .db
            .insert_one::<User>(&self.collection, User::new(email, password), None)
            .await?
            .id)
    }

    pub async fn get_user_id(&self, email: &str, password: &str) -> Result<String, RestError> {
        match self.validate_email(email, password).await {
            Ok(user) => Ok(user.id),
            Err(_) => Err(RestError::Unauthorized),
        }
    }

    pub async fn create_api_key(
        &self,
        id: &str,
        tags: Option<Vec<String>>,
    ) -> Result<ApiKey, RestError> {
        let api_key = ApiKey::new(tags);
        let filter = doc! {"id": &id };
        let update = doc! {"$push": {"api_keys": to_document(&api_key.hashed())? }};

        self.db
            .find_one_and_update::<User>(&self.collection, filter, update, None)
            .await?;
        Ok(api_key)
    }

    pub async fn delete_api_key(&self, id: &str, key: &str) -> Result<bool, RestError> {
        log::debug!("\"Trying to delete {} from {}", key, id);
        let filter = doc! {"id": &id, "api_keys.key": key };
        let update = doc! {"$pull": {"api_keys": { "key": key } }};

        match self
            .db
            .find_one_and_update::<User>(&self.collection, filter, update, None)
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    pub async fn list_api_keys(&self, id: &str) -> Result<Vec<ApiKeyBrief>, RestError> {
        let filter = doc! {"id": &id };
        let user = self
            .db
            .find_one::<User>(&self.collection, filter, None)
            .await?;
        let result: Vec<ApiKeyBrief> = user
            .api_keys
            .iter()
            .map(|s| ApiKeyBrief {
                key: s.key.to_owned(),
                created: s.created.to_owned(),
                tags: s.tags.clone(),
            })
            .collect();
        Ok(result)
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
        Ok(self
            .db
            .find_one::<User>(&self.collection, filter, None)
            .await?
            .id)
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

        self.db
            .create_indexes(&self.collection, indexes, None)
            .await
    }
}
