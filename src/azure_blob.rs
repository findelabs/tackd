use crate::error::Error as RestError;
use std::sync::Arc;
use async_trait::async_trait;
use futures::StreamExt;


use crate::trait_storage::Storage;

#[derive(Clone, Debug)]
pub struct AzureBlobClient {
    container: String,
    client: Arc<azure_storage_blobs::prelude::BlobServiceClient>
}

impl AzureBlobClient {
    pub fn new(container: &str, client: azure_storage_blobs::prelude::BlobServiceClient) -> AzureBlobClient {
        AzureBlobClient {
            container: container.to_owned(),
            client: Arc::new(client),
        }
    }
}

#[async_trait]
impl Storage for AzureBlobClient {
    async fn insert_object<'a>(
        &mut self,
        id: &'a str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<&'a str, RestError> {
        log::debug!("inserting data into Azure Blob");
        let blob_client = self.client.container_client(&self.container).blob_client(id);

        blob_client
            .put_block_blob(data)
            .content_type(content_type.to_owned())
            .await?;
        
        Ok(id)
    }

    async fn fetch_object(&self, id: &str) -> Result<Vec<u8>, RestError> {
        log::debug!("Downloading {} from azure blob", id);
        let blob_client = self.client.container_client(&self.container).blob_client(id);
        let mut complete_response = vec![];

        let mut stream = blob_client.get().chunk_size(0x2000u64).into_stream();
        while let Some(value) = stream.next().await {
            let data = value?.data.collect().await?;
            log::debug!("received {:?} bytes", data.len());
            complete_response.extend(&data);
        }

        Ok(complete_response)
    }

    async fn delete_object(&self, id: &str) -> Result<(), RestError> {
        log::debug!("Deleting {} from azure blob", id);
        let blob_client = self.client.container_client(&self.container).blob_client(id);
        // Delete value from container
        blob_client.delete().await?;
        Ok(())
    }
}
