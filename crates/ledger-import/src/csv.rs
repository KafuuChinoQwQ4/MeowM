//! Delimited-text decoding and validation helpers.
use crate::ImportError;
#[derive(Debug, Clone)]
pub(crate) struct Record {
    pub line: usize,
    pub fields: Vec<String>,
}
pub(crate) fn decode(bytes: &[u8]) -> Result<String, ImportError> {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec())
        .map_err(|_| ImportError::InvalidEncoding("账单文件不是有效的 UTF-8 编码".into()))
}
pub(crate) fn records(text: &str) -> Result<Vec<Record>, ImportError> {
    let mut out = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut after_quote = false;
    let mut line = 1;
    let mut start = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    let _ = chars.next();
                } else {
                    quoted = false;
                    after_quote = true;
                }
            } else {
                field.push(c);
                if c == '\n' {
                    line += 1;
                }
            }
        } else if after_quote {
            match c {
                ',' => {
                    fields.push(std::mem::take(&mut field));
                    after_quote = false;
                }
                '\r' => {}
                '\n' => {
                    fields.push(std::mem::take(&mut field));
                    if fields.iter().any(|f| !f.trim().is_empty()) {
                        out.push(Record {
                            line: start,
                            fields: fields.clone(),
                        });
                    }
                    fields.clear();
                    line += 1;
                    start = line;
                    after_quote = false;
                }
                _ => return Err(csv_error(line, "闭合引号后出现意外字符")),
            }
        } else {
            match c {
                '"' if field.is_empty() => quoted = true,
                '"' => return Err(csv_error(line, "未加引号字段中出现引号")),
                ',' => fields.push(std::mem::take(&mut field)),
                '\n' => {
                    fields.push(std::mem::take(&mut field));
                    if fields.iter().any(|f| !f.trim().is_empty()) {
                        out.push(Record {
                            line: start,
                            fields: fields.clone(),
                        });
                    }
                    fields.clear();
                    line += 1;
                    start = line;
                }
                '\r' => {}
                _ => field.push(c),
            }
        }
    }
    if quoted {
        return Err(csv_error(start, "文件末尾仍处于引号字段中"));
    }
    if !field.is_empty() || !fields.is_empty() {
        fields.push(field);
        if fields.iter().any(|f| !f.trim().is_empty()) {
            out.push(Record {
                line: start,
                fields,
            });
        }
    }
    Ok(out)
}
fn csv_error(line: usize, reason: &str) -> ImportError {
    ImportError::Csv(format!("CSV 第 {line} 行格式错误：{reason}"))
}
pub(crate) fn norm(s: &str) -> String {
    s.trim()
        .trim_matches('\u{feff}')
        .chars()
        .filter(|c| {
            !matches!(
                c,
                ' ' | '\t' | '\r' | '\n' | '（' | '）' | '(' | ')' | '：' | ':'
            )
        })
        .collect()
}
pub(crate) fn find_header(
    records: &[Record],
    required: &[&[&str]],
) -> Result<(usize, Vec<String>), ImportError> {
    for (i, r) in records.iter().enumerate() {
        let names = r.fields.iter().map(|x| norm(x)).collect::<Vec<_>>();
        if required
            .iter()
            .all(|a| a.iter().any(|x| names.iter().any(|n| n == &norm(x))))
        {
            return Ok((i, names));
        }
    }
    Err(ImportError::UnknownHeader(
        "无法识别账单表头：请导出包含交易时间、金额和收支字段的 CSV 文件".into(),
    ))
}
pub(crate) fn idx(names: &[String], a: &[&str]) -> Option<usize> {
    a.iter()
        .find_map(|x| names.iter().position(|n| n == &norm(x)))
}
