use crate::error::Error as RestError;
use async_trait::async_trait;
use std::sync::Arc;
use std::collections::HashMap;

use crate::storage::trait_storage::Storage;

#[derive(Clone, Debug)]
pub struct GcsClient {
    bucket: String,
    client: Arc<cloud_storage::client::Client>,
}

impl GcsClient {
    pub fn new(bucket: &str, client: cloud_storage::client::Client) -> GcsClient {
        GcsClient {
            bucket: bucket.to_owned(),
            client: Arc::new(client),
        }
    }
}

#[async_trait]
impl Storage for GcsClient {
    async fn insert_object<'a>(
        &mut self,
        id: &'a str,
        data: Vec<u8>,
        content_type: &str,
        metadata: &HashMap<String, String>
    ) -> Result<&'a str, RestError> {
        log::debug!("inserting data into GCS");
        let mut object = self.client
            .object()
            .create(&self.bucket, data, id, content_type)
            .await?;
        object.metadata = Some(metadata.clone());
        self.client.object().update(&object).await?;
        Ok(id)
    }

    async fn fetch_object(&self, id: &str) -> Result<Vec<u8>, RestError> {
        log::debug!("Downloading {} from bucket", id);
        // Get value from bucket
        match self.client.object().download(&self.bucket, id).await {
            Ok(v) => Ok(v),
            Err(e) => {
                log::error!("\"Got error attempting to fetch id from GCS: {}\"", e);
                Err(RestError::NotFound)
            }
        }
    }

    async fn delete_object(&self, id: &str) -> Result<(), RestError> {
        // Delete value from bucket
        match self.client.object().delete(&self.bucket, id).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("\"Got error attempting to fetch id from GCS: {}\"", e);
                Err(RestError::NotFound)
            }
        }
    }
}
