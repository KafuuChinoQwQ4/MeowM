use super::{OkxClient, model::*};
use crate::connectors::model::ProviderErrorCode;
use ledger_domain::crypto::{valid_decimal, valid_external_id, valid_spot_instrument};
use serde_json::Value;

impl OkxClient {
    pub(crate) fn read_tickers(&self) -> Result<Vec<Ticker>, ProviderErrorCode> {
        let rows: Vec<Value> = self.get(
            "/api/v5/market/tickers",
            &[("instType", "SPOT".into())],
            None,
        )?;
        if rows.len() > 10000 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        rows.iter()
            .map(|r| {
                let instrument = text(r, "instId")?;
                if !valid_spot_instrument(&instrument) {
                    return Err(ProviderErrorCode::InvalidResponse);
                }
                Ok(Ticker {
                    instrument,
                    last: decimal(r, "last")?,
                    open_24h: decimal(r, "open24h")?,
                    high_24h: decimal(r, "high24h")?,
                    low_24h: decimal(r, "low24h")?,
                    volume_24h: decimal(r, "vol24h")?,
                    timestamp: timestamp(r, "ts")?,
                })
            })
            .collect()
    }

    pub(crate) fn read_candles(
        &self,
        instrument: &str,
        bar: &str,
        after: Option<&str>,
    ) -> Result<Vec<Candle>, ProviderErrorCode> {
        if !valid_spot_instrument(instrument)
            || !["15m", "1H", "4H", "1D"].contains(&bar)
            || after.is_some_and(|v| !valid_external_id(v))
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let mut params = vec![
            ("instId", instrument.into()),
            ("bar", bar.into()),
            ("limit", "100".into()),
        ];
        if let Some(after) = after {
            params.push(("after", after.into()));
        }
        let path = if after.is_some() {
            "/api/v5/market/history-candles"
        } else {
            "/api/v5/market/candles"
        };
        let rows: Vec<Vec<String>> = self.get(path, &params, None)?;
        if rows.len() > 100 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let mut candles = Vec::new();
        for r in rows {
            if r.len() != 9
                || r[1..8]
                    .iter()
                    .any(|v| !valid_decimal(v) || v.starts_with('-'))
                || !["0", "1"].contains(&r[8].as_str())
            {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            let value = serde_json::json!({"ts":r[0]});
            candles.push(Candle {
                timestamp: timestamp(&value, "ts")?,
                open: r[1].clone(),
                high: r[2].clone(),
                low: r[3].clone(),
                close: r[4].clone(),
                volume: r[5].clone(),
                confirmed: r[8] == "1",
            });
        }
        candles.sort_by_key(|r| r.timestamp);
        candles.dedup_by_key(|r| r.timestamp);
        Ok(candles)
    }
}
