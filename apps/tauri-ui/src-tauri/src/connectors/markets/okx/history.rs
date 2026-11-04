use super::{OkxClient, OkxCredentials, model::*};
use crate::connectors::model::ProviderErrorCode;
use ledger_domain::crypto::{
    CryptoLeg, CryptoRecord, decimal_is_zero, decimal_product, valid_external_id,
    valid_spot_instrument,
};
use serde_json::Value;

fn fee(row: &Value) -> Result<Option<CryptoLeg>, ProviderErrorCode> {
    match optional_decimal(row, "fee")? {
        Some(quantity) if !decimal_is_zero(&quantity) => Ok(Some(CryptoLeg {
            asset: asset(row, "feeCcy")?,
            quantity,
        })),
        _ => Ok(None),
    }
}
fn negate(value: &str) -> String {
    if decimal_is_zero(value) {
        "0".into()
    } else {
        format!("-{}", value.trim_start_matches('-'))
    }
}

impl OkxClient {
    pub(crate) fn read_history(
        &self,
        credentials: &OkxCredentials<'_>,
        uid: &str,
        request: &HistoryRequest,
    ) -> Result<HistoryPage, ProviderErrorCode> {
        if !valid_external_id(uid)
            || request.begin <= 0
            || request.end < request.begin
            || request.end == i64::MAX
            || request
                .after
                .as_deref()
                .is_some_and(|v| !valid_external_id(v))
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let (path, source, id_key, time_cursor) = match request.source {
            HistorySource::Bills => ("/api/v5/account/bills", "bill", "billId", false),
            HistorySource::BillsArchive => {
                ("/api/v5/account/bills-archive", "bill", "billId", false)
            }
            HistorySource::Fills => ("/api/v5/trade/fills-history", "fill", "billId", false),
            HistorySource::Deposits => ("/api/v5/asset/deposit-history", "deposit", "depId", true),
            HistorySource::Withdrawals => (
                "/api/v5/asset/withdrawal-history",
                "withdrawal",
                "wdId",
                true,
            ),
        };
        let mut params = Vec::new();
        if request.source == HistorySource::Fills {
            params.push(("instType", "SPOT".into()));
        }
        if time_cursor {
            let after = request
                .after
                .clone()
                .unwrap_or_else(|| (request.end + 1).to_string());
            params.extend([
                ("after", after),
                ("before", (request.begin - 1).to_string()),
                ("limit", "100".into()),
            ]);
        } else {
            params.extend([
                ("begin", request.begin.to_string()),
                ("end", request.end.to_string()),
                ("limit", "100".into()),
            ]);
            if let Some(after) = &request.after {
                params.push(("after", after.clone()));
            }
        }
        let rows: Vec<Value> = self.get(path, &params, Some(credentials))?;
        if rows.len() > 100 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let fetched = rows.len();
        let mut records = Vec::new();
        let mut next_after = None;
        for row in &rows {
            let source_id = id(row, id_key)?;
            let occurred_at = timestamp(row, "ts")?;
            next_after = Some(if time_cursor {
                occurred_at.to_string()
            } else {
                source_id.clone()
            });
            if occurred_at < request.begin || occurred_at > request.end {
                continue;
            }
            let mut record = CryptoRecord {
                key: format!("okx:{uid}:{source}:{source_id}"),
                account_uid: uid.into(),
                source: source.into(),
                source_id,
                occurred_at,
                instrument: None,
                quote_asset: None,
                legs: Vec::new(),
                fee: None,
                price: None,
                platform_state: String::new(),
            };
            match source {
                "fill" => {
                    let instrument = text(row, "instId")?;
                    if !valid_spot_instrument(&instrument) {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                    let parts: Vec<_> = instrument.split('-').collect();
                    let quote_asset = match row.get("tradeQuoteCcy") {
                        None => parts[1].to_string(),
                        Some(Value::String(value)) if value.is_empty() => parts[1].to_string(),
                        _ => asset(row, "tradeQuoteCcy")?,
                    };
                    let size = decimal(row, "fillSz")?;
                    let price = decimal(row, "fillPx")?;
                    if size.starts_with('-') || price.starts_with('-') {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                    let quote = decimal_product(&size, &price)
                        .map_err(|_| ProviderErrorCode::InvalidResponse)?;
                    let side = text(row, "side")?;
                    if !["buy", "sell"].contains(&side.as_str()) {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                    record.legs = vec![
                        CryptoLeg {
                            asset: parts[0].into(),
                            quantity: if side == "buy" { size } else { negate(&size) },
                        },
                        CryptoLeg {
                            asset: quote_asset.clone(),
                            quantity: if side == "buy" { negate(&quote) } else { quote },
                        },
                    ];
                    record.instrument = Some(instrument);
                    record.quote_asset = Some(quote_asset);
                    record.price = Some(price);
                    record.fee = fee(row)?;
                    record.platform_state = "filled".into();
                }
                "bill" => {
                    record.legs.push(CryptoLeg {
                        asset: asset(row, "ccy")?,
                        quantity: decimal(row, "balChg")?,
                    });
                    record.platform_state = format!("{}/{}", id(row, "type")?, id(row, "subType")?);
                    if let Ok(instrument) = text(row, "instId")
                        && !instrument.is_empty()
                    {
                        record.instrument = Some(instrument);
                    }
                    // balChg already includes fees. Preserve the reference bill without adding a fee posting.
                }
                _ => {
                    let quantity = decimal(row, "amt")?;
                    if quantity.starts_with('-') {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                    record.legs.push(CryptoLeg {
                        asset: asset(row, "ccy")?,
                        quantity: if source == "withdrawal" {
                            negate(&quantity)
                        } else {
                            quantity
                        },
                    });
                    record.platform_state = text(row, "state")?;
                    if source == "withdrawal" {
                        record.fee = fee(row)?;
                        if let Some(fee) = &mut record.fee {
                            fee.quantity = negate(&fee.quantity);
                        }
                    }
                }
            }
            if !record.validate() {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            records.push(record);
        }
        if fetched < 100 {
            next_after = None;
        }
        if next_after.as_deref() == request.after.as_deref() && next_after.is_some() {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        Ok(HistoryPage {
            records,
            next_after,
            fetched,
        })
    }
}
