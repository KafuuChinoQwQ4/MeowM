//! Fixed currency directory and exact foreign-to-CNY conversion.

use crate::DomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrencyDefinition {
    pub code: &'static str,
    pub exponent: u8,
    pub name_zh: &'static str,
}

pub fn supported_currencies() -> &'static [CurrencyDefinition] {
    const CURRENCIES: &[CurrencyDefinition] = &[
        CurrencyDefinition {
            code: "CNY",
            exponent: 2,
            name_zh: "人民币",
        },
        CurrencyDefinition {
            code: "USD",
            exponent: 2,
            name_zh: "美元",
        },
        CurrencyDefinition {
            code: "EUR",
            exponent: 2,
            name_zh: "欧元",
        },
        CurrencyDefinition {
            code: "JPY",
            exponent: 0,
            name_zh: "日元",
        },
        CurrencyDefinition {
            code: "HKD",
            exponent: 2,
            name_zh: "港币",
        },
        CurrencyDefinition {
            code: "GBP",
            exponent: 2,
            name_zh: "英镑",
        },
        CurrencyDefinition {
            code: "KRW",
            exponent: 0,
            name_zh: "韩元",
        },
        CurrencyDefinition {
            code: "AUD",
            exponent: 2,
            name_zh: "澳元",
        },
        CurrencyDefinition {
            code: "CAD",
            exponent: 2,
            name_zh: "加元",
        },
        CurrencyDefinition {
            code: "SGD",
            exponent: 2,
            name_zh: "新加坡元",
        },
        CurrencyDefinition {
            code: "CHF",
            exponent: 2,
            name_zh: "瑞士法郎",
        },
    ];
    CURRENCIES
}

/// `rate` is foreign major units per one CNY. Positive results round half up.
pub fn convert_foreign_minor_to_cny_minor(
    amount_minor: i64,
    currency_exponent: u8,
    rate: &str,
) -> Result<i64, DomainError> {
    if amount_minor <= 0 {
        return Err(DomainError::InvalidFxAmount);
    }
    if currency_exponent > 9 {
        return Err(DomainError::InvalidCurrencyExponent);
    }
    let (whole, fraction) = match rate.split_once('.') {
        Some((whole, fraction)) if !fraction.is_empty() => (whole, fraction),
        Some(_) => return Err(DomainError::InvalidExchangeRate),
        None => (rate, ""),
    };
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(DomainError::InvalidExchangeRate);
    }
    let coefficient = whole
        .bytes()
        .chain(fraction.bytes())
        .try_fold(0_i128, |value, byte| {
            value.checked_mul(10)?.checked_add(i128::from(byte - b'0'))
        })
        .ok_or(DomainError::ArithmeticOverflow)?;
    if coefficient == 0 {
        return Err(DomainError::InvalidExchangeRate);
    }
    let scale = u32::try_from(fraction.len()).map_err(|_| DomainError::ArithmeticOverflow)?;
    let scale_factor = 10_i128
        .checked_pow(scale)
        .ok_or(DomainError::ArithmeticOverflow)?;
    let foreign_factor = 10_i128
        .checked_pow(u32::from(currency_exponent))
        .ok_or(DomainError::ArithmeticOverflow)?;
    let numerator = i128::from(amount_minor)
        .checked_mul(100)
        .and_then(|value| value.checked_mul(scale_factor))
        .ok_or(DomainError::ArithmeticOverflow)?;
    let denominator = coefficient
        .checked_mul(foreign_factor)
        .ok_or(DomainError::ArithmeticOverflow)?;
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = quotient
        .checked_add(i128::from(remainder >= denominator - remainder))
        .ok_or(DomainError::ArithmeticOverflow)?;
    i64::try_from(rounded).map_err(|_| DomainError::ArithmeticOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_foreign_minor_units_to_cny_without_float_rounding() {
        assert_eq!(
            convert_foreign_minor_to_cny_minor(10_000, 2, "7.1234"),
            Ok(1_404)
        );
        assert_eq!(convert_foreign_minor_to_cny_minor(1_000, 0, "150"), Ok(667));
    }

    #[test]
    fn rounds_exact_half_cent_up() {
        assert_eq!(convert_foreign_minor_to_cny_minor(1, 0, "200"), Ok(1));
        assert_eq!(convert_foreign_minor_to_cny_minor(1, 0, "201"), Ok(0));
    }

    #[test]
    fn invalid_and_overflowing_inputs_fail() {
        for rate in ["0", "-2", "x", "", "1.", ".5", "1e2"] {
            assert_eq!(
                convert_foreign_minor_to_cny_minor(100, 2, rate),
                Err(DomainError::InvalidExchangeRate)
            );
        }
        assert_eq!(
            convert_foreign_minor_to_cny_minor(0, 2, "7"),
            Err(DomainError::InvalidFxAmount)
        );
        assert_eq!(
            convert_foreign_minor_to_cny_minor(-1, 2, "7"),
            Err(DomainError::InvalidFxAmount)
        );
        assert_eq!(
            convert_foreign_minor_to_cny_minor(i64::MAX, 0, "0.0000000001"),
            Err(DomainError::ArithmeticOverflow)
        );
    }

    #[test]
    fn jpy_has_no_fractional_minor_unit() {
        let jpy = supported_currencies()
            .iter()
            .find(|item| item.code == "JPY")
            .unwrap();
        assert_eq!(jpy.exponent, 0);
    }

    #[test]
    fn currency_directory_covers_supported_codes_and_exponents() {
        let actual = supported_currencies()
            .iter()
            .map(|currency| (currency.code, currency.exponent))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                ("CNY", 2),
                ("USD", 2),
                ("EUR", 2),
                ("JPY", 0),
                ("HKD", 2),
                ("GBP", 2),
                ("KRW", 0),
                ("AUD", 2),
                ("CAD", 2),
                ("SGD", 2),
                ("CHF", 2),
            ]
        );
    }
}
