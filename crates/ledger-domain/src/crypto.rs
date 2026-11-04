//! Crypto quantities and review records are independent of fiat Money and postings.
use serde::{Deserialize, Serialize};

pub fn valid_asset(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

pub fn valid_spot_instrument(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    parts.len() == 2 && parts[0] != parts[1] && parts.iter().all(|p| valid_asset(p))
}

pub fn valid_external_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|c| c.is_ascii_digit())
}

fn decimal_parts(value: &str) -> Result<(i128, usize), &'static str> {
    if value.is_empty() || value.len() > 80 {
        return Err("invalid_crypto_decimal");
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.is_some_and(|f| f.is_empty() || !f.bytes().all(|c| c.is_ascii_digit()))
        || parts.next().is_some()
    {
        return Err("invalid_crypto_decimal");
    }
    let fraction = fraction.unwrap_or("").trim_end_matches('0');
    if fraction.len() > 38 {
        return Err("invalid_crypto_decimal");
    }
    let digits = format!("{whole}{fraction}");
    let coefficient = digits
        .parse::<i128>()
        .map_err(|_| "crypto_decimal_overflow")?;
    Ok((
        if value.starts_with('-') {
            -coefficient
        } else {
            coefficient
        },
        fraction.len(),
    ))
}

pub fn valid_decimal(value: &str) -> bool {
    decimal_parts(value).is_ok()
}
pub fn decimal_is_zero(value: &str) -> bool {
    decimal_parts(value).is_ok_and(|(coefficient, _)| coefficient == 0)
}

fn decimal_string(coefficient: i128, scale: usize) -> String {
    let sign = if coefficient < 0 { "-" } else { "" };
    let mut digits = coefficient.unsigned_abs().to_string();
    if scale > 0 {
        if digits.len() <= scale {
            digits = format!("{}{}", "0".repeat(scale + 1 - digits.len()), digits);
        }
        digits.insert(digits.len() - scale, '.');
        digits = digits
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }
    format!("{sign}{digits}")
}

pub fn decimal_product(left: &str, right: &str) -> Result<String, &'static str> {
    let (a, scale_a) = decimal_parts(left)?;
    let (b, scale_b) = decimal_parts(right)?;
    let coefficient = a.checked_mul(b).ok_or("crypto_decimal_overflow")?;
    if scale_a + scale_b > 38 {
        return Err("crypto_decimal_overflow");
    }
    Ok(decimal_string(coefficient, scale_a + scale_b))
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CryptoReviewStatus {
    Pending,
    Confirmed,
    Ignored,
}
impl CryptoReviewStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CryptoLeg {
    pub asset: String,
    pub quantity: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CryptoRecord {
    pub key: String,
    pub account_uid: String,
    /// bill (shared by the 7-day and archive endpoints), fill, deposit or withdrawal.
    pub source: String,
    pub source_id: String,
    pub occurred_at: i64,
    pub instrument: Option<String>,
    /// Actual quote asset may differ from the instrument suffix on unified order books.
    #[serde(default)]
    pub quote_asset: Option<String>,
    pub legs: Vec<CryptoLeg>,
    pub fee: Option<CryptoLeg>,
    pub price: Option<String>,
    /// Platform status, or type/subType for a balance bill.
    pub platform_state: String,
}

impl CryptoRecord {
    pub fn confirmable(&self) -> bool {
        !matches!(self.source.as_str(), "deposit" | "withdrawal") || self.platform_state == "2"
    }

    pub fn validate(&self) -> bool {
        if !valid_external_id(&self.account_uid)
            || !valid_external_id(&self.source_id)
            || self.key
                != format!(
                    "okx:{}:{}:{}",
                    self.account_uid, self.source, self.source_id
                )
            || self.occurred_at <= 0
            || chrono::DateTime::from_timestamp_millis(self.occurred_at).is_none()
            || self.platform_state.len() > 64
            || self.platform_state.chars().any(char::is_control)
            || self.quote_asset.as_deref().is_some_and(|v| !valid_asset(v))
            || (self.source != "fill" && self.quote_asset.is_some())
            || self
                .legs
                .iter()
                .chain(self.fee.iter())
                .any(|leg| !valid_asset(&leg.asset) || !valid_decimal(&leg.quantity))
        {
            return false;
        }
        match self.source.as_str() {
            "fill" => {
                let Some(instrument) = self
                    .instrument
                    .as_deref()
                    .filter(|v| valid_spot_instrument(v))
                else {
                    return false;
                };
                let Some(price) = self.price.as_deref() else {
                    return false;
                };
                let pair: Vec<_> = instrument.split('-').collect();
                if self.legs.len() != 2
                    || self.legs[0].asset != pair[0]
                    || self.legs[1].asset != self.quote_asset.as_deref().unwrap_or(pair[1])
                    || self.legs[0].asset == self.legs[1].asset
                    || !valid_decimal(price)
                    || price.starts_with('-')
                    || decimal_is_zero(price)
                    || self.legs.iter().any(|l| decimal_is_zero(&l.quantity))
                    || self.legs[0].quantity.starts_with('-')
                        == self.legs[1].quantity.starts_with('-')
                {
                    return false;
                }
                let quote = decimal_product(self.legs[0].quantity.trim_start_matches('-'), price);
                let actual = decimal_parts(self.legs[1].quantity.trim_start_matches('-'));
                quote.is_ok_and(|q| decimal_parts(&q) == actual)
            }
            "bill" => self.legs.len() == 1 && self.price.is_none(),
            "deposit" | "withdrawal" => {
                self.legs.len() == 1
                    && self.price.is_none()
                    && !decimal_is_zero(&self.legs[0].quantity)
                    && self.legs[0].quantity.starts_with('-') == (self.source == "withdrawal")
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReviewedCryptoRecord {
    pub record: CryptoRecord,
    pub status: CryptoReviewStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_uses_exact_decimal_quantities_including_small_assets() {
        assert_eq!(
            decimal_product("0.00000001", "98765.4321").unwrap(),
            "0.000987654321"
        );
        assert_eq!(
            decimal_product("0.00192834", "51858").unwrap(),
            "99.99985572"
        );
        assert!(decimal_product("NaN", "1").is_err());
        assert!(decimal_product("1e10", "1").is_err());
        assert!(decimal_product("99999999999999999999999999999999999999", "10").is_err());
        assert!(valid_asset("USDT"));
        assert!(valid_asset("1INCH"));
        assert!(!valid_asset("BTC?ccy=USD"));
    }
}
