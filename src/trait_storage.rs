use async_trait::async_trait;

use crate::error::Error as RestError;
use crate::gcs::GcsClient;
use crate::azure_blob::AzureBlobClient;

#[async_trait]
pub trait Storage {
    async fn insert_object<'a>(&mut self, id: &'a str, data: Vec<u8>, content_type: &str) -> Result<&'a str, RestError>;
    async fn fetch_object(&self, id: &str) -> Result<Vec<u8>, RestError>;
    async fn delete_object(&self, id:&str) -> Result<(), RestError>;
}

#[derive(Storage)]
pub enum StorageClient {
    GcsClient,
    AzureBlobClient
}

