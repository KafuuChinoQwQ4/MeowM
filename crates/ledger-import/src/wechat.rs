//! WeChat personal bill format adapter.
use crate::{
    ImportError, ImportSource, NormalizedImportRow, ParsedImport, RawImportRow, WarningCode, csv,
    digest, normalize,
};
use ledger_domain::{TransactionKind, TransactionStatus};
const REQUIRED: &[&[&str]] = &[&["交易时间"], &["金额(元)", "金额"], &["收/支"]];
pub fn parse(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    let text = csv::decode(bytes)?;
    let recs = csv::records(&text)?;
    let (h, names) = csv::find_header(&recs, REQUIRED)?;
    let ti = csv::idx(&names, &["交易时间"]).unwrap();
    let ai = csv::idx(&names, &["金额(元)", "金额"]).unwrap();
    let di = csv::idx(&names, &["收/支"]).unwrap();
    let mi = csv::idx(&names, &["交易对方"]);
    let ei = csv::idx(&names, &["交易单号"]);
    let type_i = csv::idx(&names, &["交易类型", "类型", "交易分类"]);
    let xi = csv::idx(&names, &["原交易单号"]);
    let pi = csv::idx(&names, &["商品"]);
    let payi = csv::idx(&names, &["支付方式"]);
    let si = csv::idx(&names, &["当前状态"]);
    let mut raw_rows = Vec::new();
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for r in recs.iter().skip(h + 1) {
        if r.fields
            .first()
            .map(|x| x.contains("微信支付账单") || x.starts_with("--") || x.contains("共"))
            .unwrap_or(false)
        {
            continue;
        }
        let f = &r.fields;
        let get = |i: usize| f.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        let time = get(ti);
        let amount = get(ai);
        let direction = get(di);
        let merchant = mi.map(&get).unwrap_or_default();
        let external = ei.and_then(|i| clean(get(i)));
        let original = xi.and_then(|i| clean(get(i)));
        let desc = pi.and_then(|i| clean(get(i)));
        let account = payi.and_then(|i| clean(get(i)));
        let raw = RawImportRow {
            row_number: r.line,
            time_text: time.clone(),
            amount_text: amount.clone(),
            currency_text: "CNY".into(),
            direction_text: direction.clone(),
            merchant: merchant.clone(),
            external_transaction_id: external,
            original_external_transaction_id: original,
            description: desc,
            account_hint: account,
            source_fields_digest: digest(f),
        };
        raw_rows.push(raw.clone());
        if f.len() < names.len() {
            warnings.push(normalize::warning(r.line, WarningCode::MalformedRow));
            continue;
        }
        let mut ws = Vec::new();
        let occurred = match normalize::date(&time) {
            Ok(v) => v,
            Err(c) => {
                ws.push(normalize::warning(r.line, c));
                warnings.extend(ws);
                continue;
            }
        };
        let amount_v = match normalize::amount(&amount, "CNY") {
            Ok(v) => v,
            Err(c) => {
                ws.push(normalize::warning(r.line, c));
                warnings.extend(ws);
                continue;
            }
        };
        let transaction_type = type_i.map(&get).unwrap_or_default();
        let kind = if normalize::is_transfer(&transaction_type) {
            TransactionKind::Transfer
        } else if direction == "支出" {
            TransactionKind::Expense
        } else if direction == "收入" {
            if si.map(&get).unwrap_or_default().contains("退款")
                || transaction_type.contains("退款")
            {
                TransactionKind::Refund
            } else {
                TransactionKind::Income
            }
        } else {
            warnings.push(normalize::warning(r.line, WarningCode::UnknownDirection));
            continue;
        };
        if si.map(&get).unwrap_or_default().contains("关闭")
            || si.map(&get).unwrap_or_default().contains("失败")
        {
            warnings.push(normalize::warning(
                r.line,
                WarningCode::UnsettledTransaction,
            ));
            continue;
        }
        if kind == TransactionKind::Refund && raw.original_external_transaction_id.is_none() {
            ws.push(normalize::warning(
                r.line,
                WarningCode::MissingRefundOriginal,
            ));
        }
        if kind == TransactionKind::Transfer {
            ws.push(normalize::warning(
                r.line,
                WarningCode::TransferNeedsAccountMapping,
            ));
        }
        let normalized = NormalizedImportRow {
            raw: raw.clone(),
            occurred_at: occurred,
            amount: amount_v.clone(),
            report_amount_cny: (kind != TransactionKind::Transfer).then_some(amount_v),
            kind,
            status: TransactionStatus::Pending,
            merchant: Some(merchant).filter(|s| !s.is_empty()),
            account_hint: raw.account_hint.clone(),
            warnings: ws.clone(),
        };
        warnings.extend(ws);
        rows.push(normalized)
    }
    Ok(ParsedImport {
        source: ImportSource::WeChat,
        raw_rows,
        rows,
        warnings,
    })
}
fn clean(s: String) -> Option<String> {
    let s = s.trim().to_string();
    if s.is_empty() || s == "/" {
        None
    } else {
        Some(s)
    }
}
