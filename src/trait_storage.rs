use async_trait::async_trait;
use enum_dispatch::enum_dispatch;

use crate::azure_blob::AzureBlobClient;
use crate::error::Error as RestError;
use crate::gcs::GcsClient;

#[async_trait]
#[enum_dispatch(StorageClient)]
pub trait Storage {
    async fn insert_object<'a>(
        &mut self,
        id: &'a str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<&'a str, RestError>;
    async fn fetch_object(&self, id: &str) -> Result<Vec<u8>, RestError>;
    async fn delete_object(&self, id: &str) -> Result<(), RestError>;
}

#[derive(Clone, Debug)]
#[enum_dispatch]
pub enum StorageClient {
    GcsClient(GcsClient),
    AzureBlobClient(AzureBlobClient),
}
