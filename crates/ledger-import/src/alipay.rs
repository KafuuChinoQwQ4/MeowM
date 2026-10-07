//! Alipay personal bill format adapter.
use crate::{
    ImportError, ImportSource, NormalizedImportRow, ParsedImport, RawImportRow, WarningCode, csv,
    digest, normalize,
};
use ledger_domain::{TransactionKind, TransactionStatus};
const REQUIRED: &[&[&str]] = &[
    &["交易时间", "交易创建时间"],
    &["金额", "金额（元）"],
    &["收/支"],
];
pub fn parse(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    let text = csv::decode(bytes)?;
    let recs = csv::records(&text)?;
    let (h, names) = csv::find_header(&recs, REQUIRED)?;
    let ti = csv::idx(&names, &["交易时间", "交易创建时间"]).unwrap();
    let ai = csv::idx(&names, &["金额", "金额（元）"]).unwrap();
    let di = csv::idx(&names, &["收/支"]).unwrap();
    let mi = csv::idx(&names, &["交易对方"]).or_else(|| csv::idx(&names, &["商户名称"]));
    let ei = csv::idx(&names, &["交易订单号", "交易号"]);
    let xi = csv::idx(&names, &["原交易号"]);
    let pi = csv::idx(&names, &["商品说明", "商品名称"]);
    let payi = csv::idx(&names, &["收/付款方式"]);
    let si = csv::idx(&names, &["交易状态"]);
    let ci = csv::idx(&names, &["币种"]);
    let ti_type = csv::idx(&names, &["类型", "交易类型", "交易分类"]);
    let mut raw_rows = Vec::new();
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    for r in recs.iter().skip(h + 1) {
        if r.fields
            .first()
            .map(|x| x.starts_with("--") || x.contains("共"))
            .unwrap_or(false)
        {
            continue;
        }
        let f = &r.fields;
        let get = |i: usize| f.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        let time = get(ti);
        let amount = get(ai);
        let currency = ci
            .map(&get)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "CNY".into())
            .to_uppercase();
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
            currency_text: currency.clone(),
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
                warnings.push(normalize::warning(r.line, c));
                continue;
            }
        };
        let amount_v = match normalize::amount(&amount, &currency) {
            Ok(v) => v,
            Err(c) => {
                warnings.push(normalize::warning(r.line, c));
                continue;
            }
        };
        let typ = ti_type.map(&get).unwrap_or_default();
        let kind = if normalize::is_transfer(&typ) {
            TransactionKind::Transfer
        } else if typ.contains("退款") || si.map(&get).unwrap_or_default().contains("退款") {
            TransactionKind::Refund
        } else if direction == "支出" {
            TransactionKind::Expense
        } else if direction == "收入" {
            TransactionKind::Income
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
        let report = if kind == TransactionKind::Transfer {
            ws.push(normalize::warning(
                r.line,
                WarningCode::TransferNeedsAccountMapping,
            ));
            None
        } else if currency == "CNY" {
            Some(amount_v.clone())
        } else {
            ws.push(normalize::warning(r.line, WarningCode::MissingExchangeRate));
            None
        };
        let normalized = NormalizedImportRow {
            raw: raw.clone(),
            occurred_at: occurred,
            amount: amount_v,
            report_amount_cny: report,
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
        source: ImportSource::Alipay,
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
