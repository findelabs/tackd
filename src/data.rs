use axum::body::Bytes;
use hex::encode;

use crate::error::Error as RestError;
use crate::state::Keys;

#[derive(Clone, Debug)]
pub struct Data {
    pub data: Vec<u8>,
    pub mime_type: Option<String>,
    pub key: Option<String>,
    pub encrypt_key_version: Option<u8>
}

impl Data {
    pub fn create(
        value: Bytes,
        key: Option<String>,
        keys: &Keys,
        encrypt_key: bool,
        encrypt_data: bool
    ) -> Result<Data, RestError> {
        
        // Detect binary mime-type, this could drop the debug bit in the future
        let content_type = match infer::get(&value) {
            Some(t) => {
                let mime_type = t.mime_type().to_owned();
                log::debug!("\"Detected mime type as {}\"", &mime_type);
                Some(mime_type)
            },
            None => None
        };

        // Generate random encryption key is None is passed
        let key = match key {
            Some(k) => k.to_owned(),
            None => Alphanumeric.sample_string(&mut rand::thread_rng(), 32),
        };

        let secret_key = orion::aead::SecretKey::from_slice(key.as_bytes())?;

        // Encrypt data with key
        let ciphertext = match orion::aead::seal(&secret_key, &data) {
            Ok(e) => e,
            Err(e) => {
                log::error!("Error encrypting secret: {}", e);
                return Err(RestError::CryptoError(e));
            }
        };

        // Is this is an unknown user, return "default"
        if current_user.is_none() {
            return Ok(Encryption {
                managed: false,
                key: None,
                version: None,
            });
        };

        if encrypt_key.is_some() {
            let latest_encrypt_key = keys.latest_key();
            let (_, key_encrypted) = Secret::seal(Some(&latest_encrypt_key.key), Bytes::from(key))?;
        };
