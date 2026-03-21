#[derive(Debug)]
pub enum CryptoError {
    Keychain(String),
    Encrypt,
    Decrypt,
}

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};

pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<([u8; 12], Vec<u8>), CryptoError> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce_arr = Aes256Gcm::generate_nonce(&mut OsRng);
    let nonce: [u8; 12] = nonce_arr.into();
    let ciphertext = cipher
        .encrypt(&nonce_arr, plaintext)
        .map_err(|_| CryptoError::Encrypt)?;
    Ok((nonce, ciphertext))
}

pub fn decrypt(
    key: &[u8; 32],
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new(key.into());
    let nonce_arr: aes_gcm::Nonce<_> = (*nonce).into();
    cipher
        .decrypt(&nonce_arr, ciphertext)
        .map_err(|_| CryptoError::Decrypt)
}

pub fn get_or_create_key() -> Result<[u8; 32], CryptoError> {
    let entry = keyring::Entry::new("cliphop", "history-key")
        .map_err(|e| CryptoError::Keychain(e.to_string()))?;

    match entry.get_secret() {
        Ok(bytes) if bytes.len() == 32 => {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            Ok(key)
        }
        Ok(_) => {
            // Wrong length — regenerate
            generate_and_store_key(&entry)
        }
        Err(_) => {
            // Not found (or locked) — generate new key
            generate_and_store_key(&entry)
        }
    }
}

fn generate_and_store_key(entry: &keyring::Entry) -> Result<[u8; 32], CryptoError> {
    let key_arr = Aes256Gcm::generate_key(OsRng);
    let key: [u8; 32] = key_arr.into();
    entry
        .set_secret(&key)
        .map_err(|e| CryptoError::Keychain(e.to_string()))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        [42u8; 32]
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = test_key();
        let plaintext = b"hello clipboard";
        let (nonce, ciphertext) = encrypt(&key, plaintext).expect("encrypt failed");
        let recovered = decrypt(&key, &nonce, &ciphertext).expect("decrypt failed");
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn encrypt_produces_unique_nonces() {
        let key = test_key();
        let (nonce1, _) = encrypt(&key, b"same text").unwrap();
        let (nonce2, _) = encrypt(&key, b"same text").unwrap();
        assert_ne!(nonce1, nonce2, "nonces must be unique per call");
    }

    #[test]
    fn decrypt_corrupt_ciphertext_returns_err() {
        let key = test_key();
        let nonce = [0u8; 12];
        let bad_ciphertext = vec![0u8; 32];
        let result = decrypt(&key, &nonce, &bad_ciphertext);
        assert!(matches!(result, Err(CryptoError::Decrypt)));
    }
}
