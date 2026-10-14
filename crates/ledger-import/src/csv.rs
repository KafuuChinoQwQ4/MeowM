//! Small CSV reader used for local imports. It handles quoted commas and BOMs
//! without retaining source rows after normalization.

use crate::normalize::ImportError;

pub(crate) fn decode_csv(bytes: &[u8]) -> Result<Vec<Vec<String>>, ImportError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ImportError::InvalidUtf8)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return Err(ImportError::EmptyInput);
    }
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => row.push(std::mem::take(&mut field)),
            '\n' if !quoted => {
                if field.ends_with('\r') {
                    field.pop();
                }
                row.push(std::mem::take(&mut field));
                if row.iter().any(|value| !value.trim().is_empty()) {
                    rows.push(std::mem::take(&mut row));
                } else {
                    row.clear();
                }
            }
            '\r' if !quoted && chars.peek() == Some(&'\n') => {}
            _ => field.push(ch),
        }
    }
    if quoted {
        return Err(ImportError::Csv("引号未闭合".into()));
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if row.iter().any(|value| !value.trim().is_empty()) {
            rows.push(row);
        }
    }
    if rows.is_empty() {
        return Err(ImportError::EmptyInput);
    }
    Ok(rows)
}

pub(crate) fn header_map(headers: &[String]) -> std::collections::HashMap<String, usize> {
    headers
        .iter()
        .enumerate()
        .map(|(index, value)| (normalize_header(value), index))
        .collect()
}

pub(crate) fn normalize_header(value: &str) -> String {
    value
        .trim()
        .trim_matches('\u{feff}')
        .replace([' ', '\t', '\r', '\n', '（', '）', '(', ')', '：', ':'], "")
        .to_lowercase()
}

pub(crate) fn value<'a>(
    row: &'a [String],
    headers: &std::collections::HashMap<String, usize>,
    aliases: &[&str],
) -> Option<&'a str> {
    aliases
        .iter()
        .find_map(|alias| {
            headers
                .get(&normalize_header(alias))
                .and_then(|index| row.get(*index))
        })
        .map(String::as_str)
}
