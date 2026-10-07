//! Version 1: Argon2id v1.3 (64 MiB, 3 passes, 1 lane), AES-256-GCM.
//! Salt and nonce are independent OS randomness; the complete header is AAD.
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use argon2::{Algorithm, Argon2, Params, Version};
use ledger_app::RepositoryError;
use rand::{RngCore, rngs::OsRng};
use zeroize::Zeroizing;
const MAGIC: &[u8; 8] = b"MEOWMBAK";
const HEADER: usize = 8 + 1 + 16 + 12;
fn invalid() -> RepositoryError {
    RepositoryError::InvalidData("备份密码错误、文件损坏或版本不支持".into())
}
fn key(password: &str, salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, RepositoryError> {
    if password.is_empty() || password.len() > 1024 {
        return Err(invalid());
    }
    let mut key = Zeroizing::new([0; 32]);
    let params = Params::new(65536, 3, 1, Some(32)).map_err(|_| invalid())?;
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|_| invalid())?;
    Ok(key)
}
pub fn seal(plaintext: &[u8], password: &str) -> Result<Vec<u8>, RepositoryError> {
    if password.chars().count() < 8 || plaintext.len() > crate::archive::MAX_ARCHIVE_BYTES {
        return Err(RepositoryError::InvalidData(
            "备份密码至少需要8个字符，文件不能超过64 MiB".into(),
        ));
    }
    let mut header = vec![0; HEADER];
    header[..8].copy_from_slice(MAGIC);
    header[8] = 1;
    OsRng
        .try_fill_bytes(&mut header[9..])
        .map_err(|_| invalid())?;
    let key = key(password, &header[9..25])?;
    let ciphertext = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| invalid())?
        .encrypt(
            Nonce::from_slice(&header[25..]),
            Payload {
                msg: plaintext,
                aad: &header,
            },
        )
        .map_err(|_| invalid())?;
    header.extend(ciphertext);
    Ok(header)
}
pub fn open(bytes: &[u8], password: &str) -> Result<Zeroizing<Vec<u8>>, RepositoryError> {
    if bytes.len() < HEADER + 16
        || bytes.len() > crate::archive::MAX_ARCHIVE_BYTES + HEADER + 16
        || &bytes[..8] != MAGIC
        || bytes[8] != 1
    {
        return Err(invalid());
    }
    let key = key(password, &bytes[9..25])?;
    let plaintext = Aes256Gcm::new_from_slice(key.as_ref())
        .map_err(|_| invalid())?
        .decrypt(
            Nonce::from_slice(&bytes[25..HEADER]),
            Payload {
                msg: &bytes[HEADER..],
                aad: &bytes[..HEADER],
            },
        )
        .map_err(|_| invalid())?;
    Ok(Zeroizing::new(plaintext))
}
