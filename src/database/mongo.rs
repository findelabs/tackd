//use mongodb::{Collection, IndexModel};
//use mongodb::options::{FindOptions, IndexOptions};
//use bson::{doc, from_document, to_document, Document};
use bson::Document;
use mongodb::options::{
    CreateIndexOptions, FindOneAndUpdateOptions, FindOneOptions, FindOptions, InsertOneOptions,
};
//use serde::{Deserialize, Serialize};
use futures::StreamExt;
use mongodb::IndexModel;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::Error as RestError;

#[derive(Clone, Debug)]
pub struct MongoClient {
    database: String,
    client: mongodb::Client,
}

impl MongoClient {
    pub fn new(client: mongodb::Client, database: &str) -> Self {
        MongoClient {
            database: database.to_owned(),
            client,
        }
    }

    pub async fn find_one_and_update<T: DeserializeOwned>(
        &self,
        collection: &str,
        filter: Document,
        update: Document,
        options: Option<FindOneAndUpdateOptions>,
    ) -> Result<T, RestError> {
        let collection_handle = self
            .client
            .database(&self.database)
            .collection::<T>(collection);
        log::debug!("Running find_one_and_update with filter: {}", filter);
        match collection_handle
            .find_one_and_update(filter.clone(), update.clone(), options)
            .await
        {
            Ok(v) => match v {
                Some(v) => Ok(v),
                None => {
                    log::debug!("Filter did not return any docs: {}", filter);
                    Err(RestError::NotFound)
                }
            },
            Err(e) => {
                log::error!(
                    "Error find_one_and_update: {}. filter: {} update: {}",
                    e,
                    filter,
                    update
                );
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn find_one<T: DeserializeOwned + Unpin + std::marker::Send + Sync>(
        &self,
        collection: &str,
        filter: Document,
        options: Option<FindOneOptions>,
    ) -> Result<T, RestError> {
        let collection_handle = self
            .client
            .database(&self.database)
            .collection::<T>(collection);
        log::debug!("Running find_one with filter: {}", filter);
        match collection_handle.find_one(filter.clone(), options).await {
            Ok(v) => match v {
                Some(v) => Ok(v),
                None => {
                    log::debug!("Filter did not return any docs: {}", filter);
                    Err(RestError::NotFound)
                }
            },
            Err(e) => {
                log::error!("Error find_one: {}. filter: {}", e, filter);
                Err(RestError::NotFound)
            }
        }
    }

    pub async fn find<T: DeserializeOwned + Unpin + std::marker::Send + Sync>(
        &self,
        collection: &str,
        filter: Document,
        options: Option<FindOptions>,
    ) -> Result<Vec<T>, RestError> {
        let collection_handle = self
            .client
            .database(&self.database)
            .collection::<T>(collection);
        log::debug!("Running find with filter: {}", filter);
        let mut cursor = collection_handle.find(filter.clone(), options).await?;
        let mut result: Vec<T> = Vec::new();
        while let Some(document) = cursor.next().await {
            match document {
                Ok(doc) => {
                    log::debug!("\"Found matching doc in find: {}", filter);
                    result.push(doc)
                }
                Err(e) => {
                    log::error!("Caught error querying with {}, skipping: {}", filter, e);
                    continue;
                }
            }
        }
        Ok(result)
    }

    pub async fn insert_one<
        T: DeserializeOwned + Unpin + std::marker::Send + Sync + Clone + Serialize,
    >(
        &self,
        collection: &str,
        doc: T,
        options: Option<InsertOneOptions>,
    ) -> Result<T, RestError> {
        let collection_handle = self.client.database(&self.database).collection(collection);
        match collection_handle.insert_one(doc.clone(), options).await {
            Ok(_) => Ok(doc),
            Err(e) => {
                log::error!("Error insert_one: {}", e);
                Err(RestError::BadInsert)
            }
        }
    }

    pub async fn create_indexes(
        &self,
        collection: &str,
        indexes: Vec<IndexModel>,
        options: Option<CreateIndexOptions>,
    ) -> Result<(), RestError> {
        let collection_handle = self
            .client
            .database(&self.database)
            .collection::<Document>(collection);
        match collection_handle.create_indexes(indexes, options).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("Error creating indexes: {}", e);
                Err(RestError::BadInsert)
            }
        }
    }
}
