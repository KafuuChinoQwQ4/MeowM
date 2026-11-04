use super::{OkxClient, OkxCredentials, model::*};
use crate::connectors::model::ProviderErrorCode;
use ledger_domain::crypto::{valid_asset, valid_external_id};
use serde_json::Value;

fn position(row: &Value, history: bool) -> Result<Position, ProviderErrorCode> {
    let currency = asset(row, "ccy")?;
    let instrument = text(row, "instId")?;
    if instrument.split('-').count() < 3 || !instrument.split('-').all(valid_asset) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(Position {
        position_id: id(row, "posId")?,
        instrument,
        currency,
        side: text(row, "posSide")?,
        margin_mode: text(row, "mgnMode")?,
        quantity: optional_decimal(row, if history { "closeTotalPos" } else { "pos" })?,
        open_price: optional_decimal(row, if history { "openAvgPx" } else { "avgPx" })?,
        close_price: optional_decimal(row, "closeAvgPx")?,
        unrealized_pnl: optional_decimal(row, "upl")?,
        realized_pnl: optional_decimal(row, "realizedPnl")?,
        pnl: optional_decimal(row, "pnl")?,
        fee: optional_decimal(row, "fee")?,
        funding_fee: optional_decimal(row, "fundingFee")?,
        liquidation_penalty: optional_decimal(row, "liqPenalty")?,
        updated_at: timestamp(row, "uTime")?,
    })
}
impl OkxClient {
    pub(crate) fn read_positions(
        &self,
        credentials: &OkxCredentials<'_>,
        instrument_type: &str,
        after: Option<&str>,
    ) -> Result<PositionOverview, ProviderErrorCode> {
        if !["SWAP", "FUTURES"].contains(&instrument_type)
            || after.is_some_and(|v| !valid_external_id(v))
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let current: Vec<Value> = self.get(
            "/api/v5/account/positions",
            &[("instType", instrument_type.into())],
            Some(credentials),
        )?;
        let mut params = vec![
            ("instType", instrument_type.into()),
            ("limit", "100".into()),
        ];
        if let Some(after) = after {
            params.push(("after", after.into()));
        }
        let history: Vec<Value> = self.get(
            "/api/v5/account/positions-history",
            &params,
            Some(credentials),
        )?;
        // OKX returns all rows sharing the boundary uTime, potentially exceeding limit.
        if current.len() > 1000 || history.len() > 1000 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let current = current
            .iter()
            .map(|r| position(r, false))
            .collect::<Result<Vec<_>, _>>()?;
        let history = history
            .iter()
            .map(|r| position(r, true))
            .collect::<Result<Vec<_>, _>>()?;
        let next_after = if history.len() >= 100 {
            history
                .iter()
                .map(|r| r.updated_at)
                .min()
                .map(|t| t.to_string())
        } else {
            None
        };
        if next_after.as_deref() == after && after.is_some() {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        Ok(PositionOverview {
            current,
            history,
            next_after,
            fetched_at: chrono::Utc::now().timestamp_millis(),
        })
    }
}
