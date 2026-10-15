//! Exact integer-fen parsing. Multiple, negative, foreign, or malformed amounts stay unknown.
use regex::Regex;
use std::sync::LazyLock;
pub fn parse(text: &str) -> Option<i64> {
    if ["USD", "美元", "HKD", "港币", "$", "EUR", "€", "负", "毛"]
        .iter()
        .any(|s| text.contains(s))
    {
        return None;
    }
    static AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:[￥¥]\s*)?[-−－]?[0-9][0-9.,]*\s*元?|[零〇一二两三四五六七八九十百千万壹贰叁肆伍陆柒捌玖拾佰仟]+元(?:[零〇一二两三四五六七八九壹贰叁肆伍陆柒捌玖]角)?(?:[零〇一二两三四五六七八九壹贰叁肆伍陆柒捌玖]分)?").unwrap()
    });
    let tokens: Vec<_> = AMOUNT
        .find_iter(text)
        .filter(|m| m.as_str().contains(['￥', '¥', '元']))
        .collect();
    if tokens.len() != 1 {
        return None;
    }
    let prefix = text[..tokens[0].start()].trim_end();
    if prefix.ends_with(['-', '−', '－', '.', ',']) {
        return None;
    }
    let token = tokens[0].as_str().trim();
    let amount = if token.chars().any(|c| c.is_ascii_digit()) {
        let value = token
            .trim_start_matches(['￥', '¥'])
            .trim()
            .trim_end_matches('元')
            .trim();
        let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
        if whole.is_empty() || !whole.bytes().all(|c| c.is_ascii_digit()) || fraction.len() > 2 {
            return None;
        }
        let whole: i64 = whole.parse().ok()?;
        let fraction: i64 = match fraction.len() {
            0 => 0,
            1 => fraction.parse::<i64>().ok()?.checked_mul(10)?,
            _ => fraction.parse().ok()?,
        };
        whole.checked_mul(100)?.checked_add(fraction)?
    } else {
        let (whole, fraction) = token.split_once('元')?;
        let mut amount = chinese_integer(whole)?.checked_mul(100)?;
        let mut chars = fraction.chars();
        while let Some(c) = chars.next() {
            let digit = digit(c)?;
            let unit = match chars.next()? {
                '角' => 10,
                '分' => 1,
                _ => return None,
            };
            amount = amount.checked_add(digit * unit)?;
        }
        amount
    };
    (amount > 0).then_some(amount)
}
fn digit(c: char) -> Option<i64> {
    Some(match c {
        '零' | '〇' => 0,
        '一' | '壹' => 1,
        '二' | '两' | '贰' => 2,
        '三' | '叁' => 3,
        '四' | '肆' => 4,
        '五' | '伍' => 5,
        '六' | '陆' => 6,
        '七' | '柒' => 7,
        '八' | '捌' => 8,
        '九' | '玖' => 9,
        _ => return None,
    })
}
fn chinese_integer(text: &str) -> Option<i64> {
    let mut total: i64 = 0;
    let mut number = None;
    let mut previous_unit = 100_000;
    for c in text.chars() {
        if let Some(n) = digit(c) {
            if number.is_some_and(|prior| prior != 0) {
                return None;
            }
            number = Some(n);
        } else {
            let unit = match c {
                '十' | '拾' => 10,
                '百' | '佰' => 100,
                '千' | '仟' => 1000,
                '万' => 10000,
                _ => return None,
            };
            if unit >= previous_unit {
                return None;
            }
            let n = number
                .take()
                .or_else(|| (total == 0 && unit == 10).then_some(1))?;
            if n == 0 {
                return None;
            }
            total = total.checked_add(n * unit)?;
            previous_unit = unit;
        }
    }
    // Reject ambiguous abbreviated forms such as 一百二 (120 or 102).
    if number.is_some_and(|n| n != 0) && previous_unit > 10 && total != 0 {
        return None;
    }
    total.checked_add(number.unwrap_or(0))
}
