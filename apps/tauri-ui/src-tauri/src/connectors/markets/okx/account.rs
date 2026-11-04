use super::{OkxClient, OkxCredentials, model::*};
use crate::connectors::model::ProviderErrorCode;
use ledger_domain::crypto::decimal_is_zero;
use serde_json::Value;

impl OkxClient {
    pub(crate) fn account_uid(
        &self,
        credentials: &OkxCredentials<'_>,
    ) -> Result<String, ProviderErrorCode> {
        let rows: Vec<Value> = self.get("/api/v5/account/config", &[], Some(credentials))?;
        if rows.len() != 1 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        id(&rows[0], "uid")
    }

    pub(crate) fn read_assets(
        &self,
        credentials: &OkxCredentials<'_>,
    ) -> Result<AssetOverview, ProviderErrorCode> {
        let trading: Vec<Value> = self.get("/api/v5/account/balance", &[], Some(credentials))?;
        let funding: Vec<Value> = self.get("/api/v5/asset/balances", &[], Some(credentials))?;
        let valuation: Vec<Value> = self.get(
            "/api/v5/asset/asset-valuation",
            &[("ccy", "USD".into())],
            Some(credentials),
        )?;
        if trading.len() != 1 || funding.len() > 1000 || valuation.len() != 1 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let details = trading[0]
            .get("details")
            .and_then(Value::as_array)
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        if details.len() > 1000 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let mut balances = Vec::new();
        for row in details {
            balances.push(Balance {
                account: "trading",
                asset: asset(row, "ccy")?,
                quantity: optional_decimal(row, "cashBal")?,
                available: optional_decimal(row, "availBal")?,
                frozen: optional_decimal(row, "frozenBal")?,
                equity: optional_decimal(row, "eq")?,
                equity_usd: optional_decimal(row, "eqUsd")?,
            });
        }
        for row in &funding {
            balances.push(Balance {
                account: "funding",
                asset: asset(row, "ccy")?,
                quantity: Some(decimal(row, "bal")?),
                available: Some(decimal(row, "availBal")?),
                frozen: Some(decimal(row, "frozenBal")?),
                equity: None,
                equity_usd: None,
            });
        }
        balances.retain(|b| {
            [
                &b.quantity,
                &b.available,
                &b.frozen,
                &b.equity,
                &b.equity_usd,
            ]
            .into_iter()
            .flatten()
            .any(|v| !decimal_is_zero(v))
        });
        let row = &valuation[0];
        let details = row
            .get("details")
            .ok_or(ProviderErrorCode::InvalidResponse)?;
        Ok(AssetOverview {
            balances,
            valuation: Valuation {
                total_usd: decimal(row, "totalBal")?,
                funding_usd: decimal(details, "funding")?,
                trading_usd: decimal(details, "trading")?,
                earn_usd: optional_decimal(details, "earn")?,
                timestamp: timestamp(row, "ts")?,
            },
            fetched_at: chrono::Utc::now().timestamp_millis(),
        })
    }
}
