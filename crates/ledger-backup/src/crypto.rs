use crate::BackupError;
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use zeroize::Zeroizing;
const MAGIC: &[u8; 8] = b"MEOWMBKP";
const HEADER: usize = 49; // magic(8), version(1), salt(16), nonce(24)
pub(crate) const MAX_FILE: usize = 32 * 1024 * 1024;
fn key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, BackupError> {
    if !(8..=1024).contains(&password.len()) {
        return Err(BackupError::Password);
    }
    let mut key = Zeroizing::new([0; 32]);
    let params = Params::new(19_456, 2, 1, Some(32)).map_err(|_| BackupError::Format)?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| BackupError::Format)?;
    Ok(key)
}
pub(crate) fn encrypt(data: &[u8], password: &str) -> Result<Vec<u8>, BackupError> {
    if data.len() > MAX_FILE - HEADER - 16 {
        return Err(BackupError::Format);
    }
    let mut header = [0; HEADER];
    header[..8].copy_from_slice(MAGIC);
    header[8] = 1;
    getrandom::getrandom(&mut header[9..]).map_err(|_| BackupError::Storage)?;
    let key = key(password, &header[9..25])?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(key.as_ref()).map_err(|_| BackupError::Format)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&header[25..]),
            Payload {
                msg: data,
                aad: &header,
            },
        )
        .map_err(|_| BackupError::Authentication)?;
    Ok([header.as_slice(), &ciphertext].concat())
}
pub(crate) fn decrypt(data: &[u8], password: &str) -> Result<Zeroizing<Vec<u8>>, BackupError> {
    if data.len() < HEADER + 16 || data.len() > MAX_FILE || &data[..8] != MAGIC {
        return Err(BackupError::Format);
    }
    if data[8] != 1 {
        return Err(BackupError::Version);
    }
    let key = key(password, &data[9..25])?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(key.as_ref()).map_err(|_| BackupError::Format)?;
    cipher
        .decrypt(
            XNonce::from_slice(&data[25..HEADER]),
            Payload {
                msg: &data[HEADER..],
                aad: &data[..HEADER],
            },
        )
        .map(Zeroizing::new)
        .map_err(|_| BackupError::Authentication)
}
