use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Months, NaiveDate};
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::signature::{SignatureEncoding, Signer, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey, pkcs1v15};
use sha2::Sha256;
use std::collections::BTreeMap;

pub fn wechat_canonical_request(
    method: &str,
    canonical_url: &str,
    timestamp: &str,
    nonce: &str,
    body: &str,
) -> String {
    format!("{method}\n{canonical_url}\n{timestamp}\n{nonce}\n{body}\n")
}

pub fn sign_rsa_sha256(private_key_pem: &str, message: &[u8]) -> Result<String, ()> {
    let key = parse_private_key(private_key_pem)?;
    let signing_key = pkcs1v15::SigningKey::<Sha256>::new(key);
    Ok(STANDARD.encode(signing_key.sign(message).to_bytes()))
}

pub fn verify_rsa_sha256(public_key_pem: &str, message: &[u8], signature: &str) -> Result<(), ()> {
    let key = parse_public_key(public_key_pem)?;
    let bytes = STANDARD.decode(signature).map_err(|_| ())?;
    let signature = pkcs1v15::Signature::try_from(bytes.as_slice()).map_err(|_| ())?;
    pkcs1v15::VerifyingKey::<Sha256>::new(key)
        .verify(message, &signature)
        .map_err(|_| ())
}

fn parse_private_key(pem: &str) -> Result<RsaPrivateKey, ()> {
    RsaPrivateKey::from_pkcs8_pem(pem)
        .or_else(|_| RsaPrivateKey::from_pkcs1_pem(pem))
        .map_err(|_| ())
}

fn parse_public_key(pem: &str) -> Result<RsaPublicKey, ()> {
    RsaPublicKey::from_public_key_pem(pem)
        .or_else(|_| RsaPublicKey::from_pkcs1_pem(pem))
        .map_err(|_| ())
}

pub fn alipay_canonical_parameters(parameters: &BTreeMap<String, String>) -> String {
    parameters
        .iter()
        .filter(|(name, value)| name.as_str() != "sign" && !value.is_empty())
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn validate_wechat_bill_date(value: &str, today: NaiveDate) -> Result<NaiveDate, ()> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| ())?;
    if date.to_string() != value || date >= today {
        return Err(());
    }
    let earliest = today.checked_sub_months(Months::new(3)).ok_or(())?;
    if date < earliest {
        return Err(());
    }
    Ok(date)
}

pub fn resolve_alipay_download_url(value: &str) -> Result<String, ()> {
    let mut url = url::Url::parse(value).map_err(|_| ())?;
    if url.host_str() != Some("dwbillcenter.alipay.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || !matches!(url.port(), None | Some(443))
        || !matches!(url.scheme(), "http" | "https")
    {
        return Err(());
    }
    if url.scheme() == "http" {
        url.set_scheme("https").map_err(|_| ())?;
        url.set_port(None).map_err(|_| ())?;
    }
    Ok(url.to_string())
}
