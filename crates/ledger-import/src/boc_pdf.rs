//! Bank of China transaction statement columns, including wrapped cells.

use crate::{
    ImportError, ImportWarning, MAX_IMPORT_ROWS, NormalizedImportRow, ParsedImport,
    TransferDirection, normalize,
    pdf::{Page, Word},
};
use ledger_domain::{SourceKind, TransactionKind};

const HEADERS: [&str; 12] = [
    "记账日期",
    "记账时间",
    "币别",
    "金额",
    "余额",
    "交易名称",
    "渠道",
    "网点名称",
    "附言",
    "对方账户名",
    "对方卡号/账号",
    "对方开户行",
];

struct Line<'a> {
    y: f64,
    words: Vec<&'a Word>,
}

fn lines(page: &Page) -> Vec<Line<'_>> {
    let mut words: Vec<_> = page.words.iter().collect();
    words.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<Line<'_>> = Vec::new();
    for word in words {
        if let Some(line) = lines
            .last_mut()
            .filter(|line| (line.y - word.y).abs() <= 1.5)
        {
            line.words.push(word);
        } else {
            lines.push(Line {
                y: word.y,
                words: vec![word],
            });
        }
    }
    for line in &mut lines {
        line.words.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    lines
}

fn cells(line: &Line<'_>, boundaries: &[f64]) -> [String; 12] {
    let mut cells = std::array::from_fn(|_| String::new());
    for word in &line.words {
        let center = (word.x + word.right) / 2.;
        let column = boundaries.partition_point(|boundary| center >= *boundary);
        if column < cells.len() {
            cells[column].push_str(&word.text);
        }
    }
    cells
}

pub(crate) fn parse(pages: &[Page]) -> Result<ParsedImport, ImportError> {
    let mut parsed = ParsedImport {
        source_kind: SourceKind::Bank,
        rows: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };
    let mut ordinal = 0;
    let mut balances = Vec::new();
    for page in pages {
        let lines = lines(page);
        if !page
            .words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<String>()
            .contains("中国银行交易流水明细清单")
        {
            return Err(ImportError::Pdf(format!(
                "第 {} 页不是可识别的中国银行交易流水明细清单；扫描件和其他版式暂不支持",
                page.number
            )));
        }
        let (header, centers) = lines
            .iter()
            .find_map(|line| {
                let centers: Option<Vec<_>> = HEADERS
                    .iter()
                    .map(|header| {
                        line.words
                            .iter()
                            .find(|word| word.text.trim() == *header)
                            .map(|word| (word.x + word.right) / 2.)
                    })
                    .collect();
                centers.map(|centers| (line, centers))
            })
            .ok_or_else(|| {
                ImportError::Pdf(format!(
                    "第 {} 页缺少日期、金额等完整表头，请检查 PDF 版式",
                    page.number
                ))
            })?;
        if !centers.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(ImportError::Pdf("账单表格列顺序无法识别".into()));
        }
        let boundaries: Vec<_> = centers
            .windows(2)
            .map(|pair| (pair[0] + pair[1]) / 2.)
            .collect();
        let data: Vec<_> = lines
            .iter()
            .filter(|line| line.y > header.y + 2. && line.y < page.height - 30.)
            .map(|line| (line.y, cells(line, &boundaries)))
            .collect();
        let mut records: Vec<[String; 12]> = Vec::new();
        for (_, cells) in data {
            let date = cells[0].trim();
            let time = cells[1].trim();
            let is_date = date.len() == 10
                && date.as_bytes().get(4) == Some(&b'-')
                && date.as_bytes().get(7) == Some(&b'-');
            let is_time = time.len() == 8
                && time.as_bytes().get(2) == Some(&b':')
                && time.as_bytes().get(5) == Some(&b':');
            let amount_present = statement_amount(&cells[3], 0).is_ok();
            let balance_present = statement_amount(&cells[4], 0).is_ok();
            if is_date || is_time || amount_present || balance_present {
                records.push(cells);
            } else if let Some(record) = records.last_mut() {
                // Only table cells can continue; page footers and balance summaries
                // outside the table are never merged into a transaction.
                for column in [5, 6, 7, 8, 9, 10, 11] {
                    record[column].push_str(&cells[column]);
                }
            }
        }
        if records.is_empty() {
            return Err(ImportError::Pdf(format!(
                "第 {} 页没有可读取的流水，请检查是否为扫描件",
                page.number
            )));
        }
        for (index, record) in records.iter().enumerate() {
            ordinal += 1;
            if ordinal > MAX_IMPORT_ROWS {
                return Err(ImportError::LimitExceeded(
                    "每次最多导入 10,000 行，请拆分文件".into(),
                ));
            }
            match normalize_row(record, ordinal, page.number, index + 1) {
                Ok(row) => {
                    let (signed, _) = statement_amount(&record[3], ordinal)?;
                    let (balance, _) = statement_amount(&record[4], ordinal)?;
                    balances.push((ordinal, row.occurred_at, signed, balance));
                    parsed.rows.push(row);
                }
                Err(error) => parsed.errors.push(ImportWarning {
                    row: ordinal,
                    code: "invalid_pdf_row".into(),
                    message: format!("PDF 第 {} 页第 {} 笔：{error}", page.number, index + 1),
                }),
            }
        }
    }
    let ascending = balances.windows(2).all(|pair| pair[0].1 <= pair[1].1);
    let descending = balances.windows(2).all(|pair| pair[0].1 >= pair[1].1);
    if ascending || descending {
        for pair in balances
            .windows(2)
            .filter(|pair| pair[1].0 == pair[0].0 + 1)
        {
            let (before, after) = if ascending {
                (&pair[0], &pair[1])
            } else {
                (&pair[1], &pair[0])
            };
            if after.3.checked_sub(before.3) != Some(after.2) {
                parsed.warnings.push(ImportWarning {
                    row: pair[1].0,
                    code: "pdf_balance_mismatch".into(),
                    message:
                        "PDF 相邻流水的余额差与交易金额不一致，请对照原文件核对是否缺行或金额异常。"
                            .into(),
                });
            }
        }
    }
    Ok(parsed)
}

fn statement_amount(value: &str, row: usize) -> Result<(i64, bool), ImportError> {
    let valid = value.trim().rsplit_once('.').is_some_and(|(_, fraction)| {
        fraction.len() == 2 && fraction.bytes().all(|byte| byte.is_ascii_digit())
    });
    if !valid {
        return Err(ImportError::InvalidAmount {
            row,
            value: value.into(),
        });
    }
    normalize::parse_minor(value, 2, row)
}

fn normalize_row(
    cells: &[String; 12],
    row: usize,
    page: u32,
    position: usize,
) -> Result<NormalizedImportRow, ImportError> {
    if cells[2].trim() != "人民币" {
        return Err(ImportError::Pdf(
            "目前仅支持人民币流水，请按币种导出".into(),
        ));
    }
    let occurred_at =
        normalize::parse_date(&format!("{} {}", cells[0].trim(), cells[1].trim()), row)?;
    let (signed, negative) = statement_amount(&cells[3], row)?;
    let amount_minor = signed
        .checked_abs()
        .filter(|amount| *amount > 0)
        .ok_or_else(|| ImportError::InvalidAmount {
            row,
            value: cells[3].clone(),
        })?;
    // Validate the balance column independently so a shifted table cannot silently
    // mistake a balance for an amount.
    statement_amount(&cells[4], row)?;
    let transaction = cells[5].trim();
    let note = normalize::normalize_text(Some(&cells[8]));
    let channel = normalize::normalize_text(Some(&cells[6]));
    let merchant = normalize::normalize_text(Some(&cells[9]));
    let text = format!(
        "{}{}{}",
        transaction,
        note.as_deref().unwrap_or_default(),
        merchant.as_deref().unwrap_or_default()
    );
    let transfer = !["手续费", "服务费"].iter().any(|key| text.contains(key))
        && ([
            "提现",
            "取现",
            "自助存款",
            "现金存入",
            "现金支取",
            "转账",
            "划转",
            "汇款",
            "转存",
            "转入",
            "转出",
        ]
        .iter()
        .any(|key| text.contains(key))
            || (["支付宝", "财付通", "微信", "花呗", "余额宝"]
                .iter()
                .any(|key| text.contains(key))
                && ["充值", "还款"].iter().any(|key| text.contains(key))));
    let kind = if transaction.contains("退款") && !negative {
        TransactionKind::Refund
    } else if transfer {
        TransactionKind::Transfer
    } else if negative {
        TransactionKind::Expense
    } else {
        TransactionKind::Income
    };
    let description = normalize::normalize_text(Some(
        &[
            Some(transaction).filter(|value| !value.is_empty()),
            channel.as_deref(),
            note.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · "),
    ));
    let currency = normalize::cny();
    let digest = NormalizedImportRow::digest_for(
        SourceKind::Bank,
        None,
        occurred_at,
        kind,
        amount_minor,
        &currency,
        merchant.as_deref(),
        description.as_deref(),
    );
    Ok(NormalizedImportRow {
        row_number: row,
        source_kind: SourceKind::Bank,
        external_transaction_id: None,
        merchant_order_id: None,
        original_external_transaction_id: None,
        occurred_at,
        kind,
        transfer_direction: (kind == TransactionKind::Transfer).then_some(if negative {
            TransferDirection::Outgoing
        } else {
            TransferDirection::Incoming
        }),
        amount_minor,
        currency,
        merchant,
        description,
        account_hint: Some("中国银行".into()),
        source_fields_digest: digest,
        warnings: vec![ImportWarning {
            row,
            code: "pdf_location".into(),
            message: format!(
                "PDF 第 {page} 页第 {position} 笔；原文件未提供交易流水号，按时间、金额和摘要核对。"
            ),
        }]
        .into_iter()
        .chain((kind == TransactionKind::Transfer).then(|| {
            ImportWarning {
                row,
                code: "transfer_accounts_required".into(),
                message:
                    "这笔记录是充值、提现或还款，请按实际流向选择转出、转入平台，避免重复计收支。"
                        .into(),
            }
        }))
        .collect(),
    })
}
