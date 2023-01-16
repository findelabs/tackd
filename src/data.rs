use axum::body::Bytes;
use rand::distributions::{Alphanumeric, DistString};

use crate::error::Error as RestError;
use crate::state::Keys;

#[derive(Clone, Debug)]
pub struct Data {
    pub data: Vec<u8>,
    pub mime_type: Option<String>,
    pub key: Option<String>,
    pub encrypted_key: Option<Vec<u8>>,
    pub encrypted_key_version: Option<u8>,
}

impl Data {
    pub fn encrypt(key: String, value: Bytes) -> Result<Vec<u8>, RestError> {
        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;

        // Encrypt data with key
        let ciphertext = match orion::aead::seal(&secret_key, &value) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e));
            }
        };

        Ok(ciphertext)
    }

    pub fn create(
        value: Bytes,
        key: Option<String>,
        keys: &Keys,
        encrypt_key: bool,
        encrypt_data: bool,
    ) -> Result<Data, RestError> {
        // Detect binary mime-type, this could drop the debug bit in the future
        let content_type = match infer::get(&value) {
            Some(t) => {
                let mime_type = t.mime_type().to_owned();
                log::debug!("\"Detected mime type as {}\"", &mime_type);
                Some(mime_type)
            }
            None => None,
        };

        if encrypt_data {
            log::debug!("Data payload is being encrypted");
            // Generate random encryption key is None is passed
            let key = match key {
                Some(k) => k,
                None => Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
            };

            // Encrypt data Bytes
            let ciphertext = Data::encrypt(key.clone(), value)?;

            if encrypt_key {
                log::debug!("Encryption key is being encrypted");
                let latest_encrypt_key = keys.latest_key();
                let encrypted_key =
                    Data::encrypt(latest_encrypt_key.key.clone(), Bytes::from(key))?;

                Ok(Data {
                    data: ciphertext,
                    mime_type: content_type,
                    key: Some(latest_encrypt_key.key),
                    encrypted_key: Some(encrypted_key),
                    encrypted_key_version: Some(latest_encrypt_key.ver),
                })
            } else {
                Ok(Data {
                    data: ciphertext,
                    mime_type: content_type,
                    key: Some(key),
                    encrypted_key: None,
                    encrypted_key_version: None,
                })
            }
        } else {
            log::debug!("Data payload is NOT being encrypted");
            // Return data unchanged
            Ok(Data {
                data: value.into(),
                mime_type: content_type,
                key: None,
                encrypted_key: None,
                encrypted_key_version: None,
            })
        }
    }
}
