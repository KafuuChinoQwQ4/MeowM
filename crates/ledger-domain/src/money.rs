//! Currency and integer minor-unit money types.

use crate::DomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CurrencyCode {
    code: String,
    minor_unit_exponent: u8,
}

impl CurrencyCode {
    pub fn new(code: &str, minor_unit_exponent: u8) -> Result<Self, DomainError> {
        if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(DomainError::InvalidCurrencyCode);
        }
        if minor_unit_exponent > 9 {
            return Err(DomainError::InvalidCurrencyExponent);
        }
        Ok(Self {
            code: code.to_owned(),
            minor_unit_exponent,
        })
    }

    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn minor_unit_exponent(&self) -> u8 {
        self.minor_unit_exponent
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Money {
    minor_units: i64,
    currency: CurrencyCode,
}

impl Money {
    pub fn new(minor_units: i64, currency: CurrencyCode) -> Self {
        Self {
            minor_units,
            currency,
        }
    }
    pub fn minor_units(&self) -> i64 {
        self.minor_units
    }
    pub fn currency(&self) -> &CurrencyCode {
        &self.currency
    }

    pub fn checked_add(&self, other: Self) -> Result<Self, DomainError> {
        self.ensure_same_currency(&other)?;
        self.minor_units
            .checked_add(other.minor_units)
            .map(|minor_units| Self::new(minor_units, self.currency.clone()))
            .ok_or(DomainError::ArithmeticOverflow)
    }

    pub fn checked_sub(&self, other: Self) -> Result<Self, DomainError> {
        self.ensure_same_currency(&other)?;
        self.minor_units
            .checked_sub(other.minor_units)
            .map(|minor_units| Self::new(minor_units, self.currency.clone()))
            .ok_or(DomainError::ArithmeticOverflow)
    }

    pub fn decimal_string(&self) -> String {
        let exponent = self.currency.minor_unit_exponent as u32;
        let magnitude = self.minor_units.unsigned_abs();
        if exponent == 0 {
            return format!(
                "{}{}",
                if self.minor_units < 0 { "-" } else { "" },
                magnitude
            );
        }
        let scale = 10_u64.pow(exponent);
        format!(
            "{}{}.{:0width$}",
            if self.minor_units < 0 { "-" } else { "" },
            magnitude / scale,
            magnitude % scale,
            width = exponent as usize
        )
    }

    fn ensure_same_currency(&self, other: &Self) -> Result<(), DomainError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(DomainError::CurrencyMismatch)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CurrencyCode, Money};
    use crate::DomainError;

    #[test]
    fn money_keeps_minor_units_without_float_rounding() {
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        let amount = Money::new(1234, cny);
        assert_eq!(amount.minor_units(), 1234);
        assert_eq!(amount.decimal_string(), "12.34");
    }

    #[test]
    fn money_rejects_arithmetic_across_currency_codes() {
        let cny = Money::new(100, CurrencyCode::new("CNY", 2).unwrap());
        let usd = Money::new(100, CurrencyCode::new("USD", 2).unwrap());
        assert!(cny.checked_add(usd).is_err());
    }

    #[test]
    fn zero_exponent_currency_formats_without_decimal_places() {
        let jpy = Money::new(1234, CurrencyCode::new("JPY", 0).unwrap());
        assert_eq!(jpy.decimal_string(), "1234");
    }

    #[test]
    fn currency_code_requires_three_uppercase_ascii_letters_and_supported_exponent() {
        assert!(matches!(
            CurrencyCode::new("cnY", 2),
            Err(DomainError::InvalidCurrencyCode)
        ));
        assert!(matches!(
            CurrencyCode::new("US", 2),
            Err(DomainError::InvalidCurrencyCode)
        ));
        assert!(matches!(
            CurrencyCode::new("USD", 10),
            Err(DomainError::InvalidCurrencyExponent)
        ));
    }

    #[test]
    fn money_formats_zero_and_negative_minor_units() {
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        assert_eq!(Money::new(0, cny.clone()).decimal_string(), "0.00");
        assert_eq!(Money::new(-105, cny).decimal_string(), "-1.05");
    }

    #[test]
    fn money_arithmetic_detects_overflow() {
        let cny = CurrencyCode::new("CNY", 2).unwrap();
        let max = Money::new(i64::MAX, cny.clone());
        let one = Money::new(1, cny);
        assert_eq!(max.checked_add(one), Err(DomainError::ArithmeticOverflow));
    }
}
