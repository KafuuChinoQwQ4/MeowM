use super::crypto::{
    alipay_canonical_parameters, resolve_alipay_download_url, sign_rsa_sha256, verify_rsa_sha256,
};
use super::http::{
    MAX_BILL_BYTES, MAX_JSON_BYTES, agent, map_http_status, map_ureq_error, read_bounded,
};
use crate::connectors::model::ProviderErrorCode;
use chrono::{Local, NaiveDate};
use serde_json::Value;
use std::collections::BTreeMap;
use zeroize::{Zeroize, Zeroizing};

const ALIPAY_GATEWAY: &str = "https://openapi.alipay.com/gateway.do";
const METHOD: &str = "alipay.data.dataservice.bill.downloadurl.query";
const RESPONSE_KEY: &str = "alipay_data_dataservice_bill_downloadurl_query_response";

#[derive(Clone, Copy)]
pub struct AlipayCredentials<'a> {
    pub app_id: &'a str,
    pub app_private_key_pem: &'a str,
    pub alipay_public_key_pem: &'a str,
}

pub struct AlipayClient {
    agent: ureq::Agent,
    gateway: String,
}

impl Default for AlipayClient {
    fn default() -> Self {
        Self::new(agent())
    }
}

impl AlipayClient {
    pub fn new(agent: ureq::Agent) -> Self {
        Self {
            agent,
            gateway: ALIPAY_GATEWAY.to_string(),
        }
    }

    #[cfg(test)]
    pub(super) fn for_test(agent: ureq::Agent, gateway: String) -> Self {
        Self { agent, gateway }
    }

    pub fn read_bill(
        &self,
        bill_date: &str,
        credentials: AlipayCredentials<'_>,
    ) -> Result<(String, u64), ProviderErrorCode> {
        let today = Local::now().date_naive();
        validate_bill_date(bill_date, today)?;
        let bill_url = self.request_bill_url(bill_date, today, &credentials)?;
        let bytes = self.download_bill(&bill_url)?;
        Ok((bill_date.to_string(), bytes.len() as u64))
    }

    pub(super) fn request_bill_url(
        &self,
        bill_date: &str,
        today: NaiveDate,
        credentials: &AlipayCredentials<'_>,
    ) -> Result<Zeroizing<String>, ProviderErrorCode> {
        validate_bill_date(bill_date, today)?;
        if credentials.app_id.is_empty() || credentials.app_id.len() > 64 {
            return Err(ProviderErrorCode::InvalidCredentials);
        }

        let biz_content = serde_json::json!({
            "bill_date": bill_date,
            "bill_type": "trade"
        })
        .to_string();
        let mut parameters = BTreeMap::from([
            ("app_id".to_string(), credentials.app_id.to_string()),
            ("biz_content".to_string(), biz_content),
            ("charset".to_string(), "utf-8".to_string()),
            ("method".to_string(), METHOD.to_string()),
            ("sign_type".to_string(), "RSA2".to_string()),
            (
                "timestamp".to_string(),
                chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ),
            ("version".to_string(), "1.0".to_string()),
        ]);
        let canonical = alipay_canonical_parameters(&parameters);
        let signature = sign_rsa_sha256(credentials.app_private_key_pem, canonical.as_bytes())
            .map_err(|_| ProviderErrorCode::InvalidCredentials)?;
        parameters.insert("sign".to_string(), signature);
        let mut form = form_encode(&parameters);
        let response_result = self
            .agent
            .post(&self.gateway)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=utf-8",
            )
            .header("Accept", "application/json")
            .send(form.as_bytes());
        form.zeroize();
        for value in parameters.values_mut() {
            value.zeroize();
        }
        let mut response = response_result.map_err(map_ureq_error)?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(map_http_status(response.status().as_u16()));
        }
        let body = read_bounded(&mut response, MAX_JSON_BYTES)?;
        let mut envelope: Value =
            serde_json::from_slice(&body).map_err(|_| ProviderErrorCode::InvalidResponse)?;
        let signature = envelope
            .get("sign")
            .and_then(Value::as_str)
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        let canonical_response = Zeroizing::new(canonical_response_parameters(&envelope)?);
        verify_rsa_sha256(
            credentials.alipay_public_key_pem,
            canonical_response.as_bytes(),
            signature,
        )
        .map_err(|_| ProviderErrorCode::InvalidResponse)?;
        if let Some(Value::String(mut signature)) = envelope
            .as_object_mut()
            .and_then(|object| object.remove("sign"))
        {
            signature.zeroize();
        }

        let response = envelope
            .get_mut(RESPONSE_KEY)
            .and_then(Value::as_object_mut)
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        let code = response
            .get("code")
            .and_then(Value::as_str)
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        if code != "10000" {
            return Err(if matches!(code, "ACQ.NO_DATA" | "ACQ.NO_BILL") {
                ProviderErrorCode::BillUnavailable
            } else {
                ProviderErrorCode::ProviderFailure
            });
        }
        let mut url = match response.remove("bill_download_url") {
            Some(Value::String(url)) => Zeroizing::new(url),
            _ => return Err(ProviderErrorCode::InvalidResponse),
        };
        let secure_url = Zeroizing::new(
            resolve_alipay_download_url(&url).map_err(|_| ProviderErrorCode::InvalidResponse)?,
        );
        url.zeroize();
        Ok(secure_url)
    }

    fn download_bill(&self, bill_url: &str) -> Result<Zeroizing<Vec<u8>>, ProviderErrorCode> {
        let secure_url = Zeroizing::new(
            resolve_alipay_download_url(bill_url)
                .map_err(|_| ProviderErrorCode::InvalidResponse)?,
        );
        let mut response = self
            .agent
            .get(secure_url.as_str())
            .call()
            .map_err(map_ureq_error)?;
        if !(200..300).contains(&response.status().as_u16()) {
            return Err(map_http_status(response.status().as_u16()));
        }
        read_bounded(&mut response, MAX_BILL_BYTES)
    }
}

fn validate_bill_date(value: &str, today: NaiveDate) -> Result<(), ProviderErrorCode> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ProviderErrorCode::InvalidRequest)?;
    if date.to_string() != value || date >= today {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(())
}

fn canonical_response_parameters(envelope: &Value) -> Result<String, ProviderErrorCode> {
    let object = envelope
        .as_object()
        .ok_or(ProviderErrorCode::InvalidResponse)?;
    let mut parameters = BTreeMap::new();
    for (name, value) in object {
        if name == "sign" || value.is_null() {
            continue;
        }
        let value = match value {
            Value::String(value) => value.clone(),
            value => {
                serde_json::to_string(value).map_err(|_| ProviderErrorCode::InvalidResponse)?
            }
        };
        parameters.insert(name.clone(), value);
    }
    Ok(alipay_canonical_parameters(&parameters))
}

fn form_encode(parameters: &BTreeMap<String, String>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(parameters.iter());
    serializer.finish()
}
