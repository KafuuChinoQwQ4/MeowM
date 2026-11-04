use super::binance::decimal_is_nonzero;
use super::{default_agent, map_exchange_http_status, parse_json_response, request_error};
use crate::connectors::model::{AssetBalance, ProviderErrorCode};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{SecondsFormat, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use ureq::Agent;

mod account;
mod history;
mod market;
pub(crate) mod model;
mod order_validation;
mod positions;
#[cfg(test)]
mod tests;
use self::model::OkxEnvironment;

const API_BASE: &str = "https://openapi.okx.com";
const BALANCE_PATH: &str = "/api/v5/account/balance";
const TRADE_POST_PATHS: &[&str] = &["/api/v5/trade/order", "/api/v5/trade/cancel-order"];
const PENDING_ORDERS_PAGE_SIZE: usize = 100;
const MAX_PENDING_ORDERS_PER_INSTRUMENT: usize = 500;

pub(crate) struct OkxCredentials<'a> {
    pub api_key: &'a str,
    pub secret_key: &'a str,
    pub passphrase: &'a str,
}

pub(crate) struct OkxClient {
    agent: Agent,
    base_url: String,
}

impl Default for OkxClient {
    fn default() -> Self {
        Self {
            agent: default_agent(),
            base_url: API_BASE.to_string(),
        }
    }
}

impl OkxClient {
    fn post<T, B>(
        &self,
        path: &'static str,
        body: &B,
        credentials: &OkxCredentials<'_>,
        environment: OkxEnvironment,
    ) -> Result<T, ProviderErrorCode>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize + ?Sized,
    {
        if !TRADE_POST_PATHS.contains(&path) {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let body = serde_json::to_vec(body).map_err(|_| ProviderErrorCode::InvalidRequest)?;
        let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut prehash = format!("{timestamp}POST{path}").into_bytes();
        prehash.extend_from_slice(&body);
        let signature = hmac_base64(credentials.secret_key.as_bytes(), &prehash)?;
        let url = format!("{}{path}", self.base_url.trim_end_matches('/'));
        let mut request = self
            .agent
            .post(&url)
            .header("Content-Type", "application/json")
            .header("OK-ACCESS-KEY", credentials.api_key)
            .header("OK-ACCESS-SIGN", &signature)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", credentials.passphrase);
        if environment == OkxEnvironment::Demo {
            request = request.header("x-simulated-trading", "1");
        }
        let mut response = request.send(body.as_slice()).map_err(request_error)?;
        let value = parse_json_response(&mut response, map_exchange_http_status)?;
        parse_okx_envelope(value)
    }

    fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
        credentials: Option<&OkxCredentials<'_>>,
    ) -> Result<T, ProviderErrorCode> {
        self.get_with_environment(path, params, credentials, OkxEnvironment::Production)
    }

    fn get_with_environment<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
        credentials: Option<&OkxCredentials<'_>>,
        environment: OkxEnvironment,
    ) -> Result<T, ProviderErrorCode> {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())))
            .finish();
        let request_path = if query.is_empty() {
            path.to_string()
        } else {
            format!("{path}?{query}")
        };
        let url = format!("{}{request_path}", self.base_url.trim_end_matches('/'));
        let mut request = self.agent.get(&url);
        if let Some(credentials) = credentials {
            let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
            let signature = hmac_base64(
                credentials.secret_key.as_bytes(),
                format!("{timestamp}GET{request_path}").as_bytes(),
            )?;
            request = request
                .header("OK-ACCESS-KEY", credentials.api_key)
                .header("OK-ACCESS-SIGN", &signature)
                .header("OK-ACCESS-TIMESTAMP", &timestamp)
                .header("OK-ACCESS-PASSPHRASE", credentials.passphrase);
            if environment == OkxEnvironment::Demo {
                request = request.header("x-simulated-trading", "1");
            }
        }
        let mut response = request.call().map_err(request_error)?;
        let value = parse_json_response(&mut response, map_exchange_http_status)?;
        parse_okx_envelope(value)
    }
    #[cfg(test)]
    pub(super) fn for_test(agent: Agent, base_url: String) -> Self {
        Self { agent, base_url }
    }

    pub(super) fn read_balances(
        &self,
        credentials: &OkxCredentials<'_>,
    ) -> Result<Vec<AssetBalance>, ProviderErrorCode> {
        let accounts: Vec<OkxAccountBalance> = self.get(BALANCE_PATH, &[], Some(credentials))?;

        let mut balances = Vec::new();
        for account in accounts {
            for detail in account.details {
                validate_currency(&detail.currency)?;
                let nonzero =
                    decimal_is_nonzero(&detail.available)? || decimal_is_nonzero(&detail.frozen)?;
                if nonzero {
                    balances.push(AssetBalance {
                        asset: detail.currency,
                        free: detail.available,
                        locked: detail.frozen,
                    });
                    if balances.len() > 1000 {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                }
            }
        }
        Ok(balances)
    }

    pub(crate) fn read_instrument_spec(
        &self,
        inst_type: model::OrderInstrumentType,
        inst_id: &str,
    ) -> Result<model::InstrumentSpec, ProviderErrorCode> {
        if !order_validation::valid_instrument_id(inst_type, inst_id) {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let params = [
            ("instType", inst_type.as_okx().to_string()),
            ("instId", inst_id.to_string()),
        ];
        let rows: Vec<serde_json::Value> = self.get("/api/v5/public/instruments", &params, None)?;
        if rows.len() != 1 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let spec = order_validation::parse_instrument_spec(inst_type, &rows[0])?;
        if spec.inst_id != inst_id {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        Ok(spec)
    }

    pub(crate) fn read_trade_context(
        &self,
        credentials: &OkxCredentials<'_>,
        inst_type: model::OrderInstrumentType,
        inst_id: &str,
        environment: OkxEnvironment,
    ) -> Result<model::AccountTradeContext, ProviderErrorCode> {
        use model::{
            AccountTradeContext, InstrumentLeverage, OrderInstrumentType, PositionMode, TradeMode,
        };

        if !order_validation::valid_instrument_id(inst_type, inst_id) {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        let mut params = Vec::new();
        if inst_type != OrderInstrumentType::Spot {
            params.push(("instType", inst_type.as_okx().to_string()));
        }
        params.push(("instId", inst_id.to_string()));

        let config: Vec<serde_json::Value> = self.get_with_environment(
            "/api/v5/account/config",
            &[],
            Some(credentials),
            environment,
        )?;
        if config.len() != 1 {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let config = &config[0];
        let acct_lv = model::text(config, "acctLv")?;
        if !matches!(acct_lv.as_str(), "1" | "2" | "3" | "4") {
            return Err(ProviderErrorCode::InvalidResponse);
        }
        let pos_mode = match model::text(config, "posMode")?.as_str() {
            "net_mode" => PositionMode::Net,
            "long_short_mode" => PositionMode::LongShort,
            _ => return Err(ProviderErrorCode::InvalidResponse),
        };

        let positions = if inst_type == OrderInstrumentType::Spot {
            Vec::new()
        } else {
            let rows: Vec<serde_json::Value> = self.get_with_environment(
                "/api/v5/account/positions",
                &params,
                Some(credentials),
                environment,
            )?;
            if rows.len() > 1000 {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            rows.iter()
                .map(order_validation::parse_position)
                .collect::<Result<Vec<_>, _>>()?
        };

        // Pending-close accounting is used only for contracts. OKX reports
        // spot `accFillSz` in base currency while `sz` may use quote currency,
        // so a generic subtraction would not produce a meaningful remainder.
        let pending_orders = if inst_type == OrderInstrumentType::Spot {
            Vec::new()
        } else {
            self.read_pending_orders(credentials, inst_type, inst_id, environment)?
        };

        let mut leverage = Vec::new();
        if inst_type != OrderInstrumentType::Spot {
            for margin_mode in [TradeMode::Cross, TradeMode::Isolated] {
                let leverage_params = [
                    ("instId", inst_id.to_string()),
                    ("mgnMode", margin_mode.as_okx().to_string()),
                ];
                let rows: Vec<serde_json::Value> = match self.get_with_environment(
                    "/api/v5/account/leverage-info",
                    &leverage_params,
                    Some(credentials),
                    environment,
                ) {
                    Ok(rows) => rows,
                    // Leverage is display-only context; some account modes do not
                    // expose it, so an unavailable read must not block order review.
                    Err(_) => continue,
                };
                if rows.len() > 1000 {
                    return Err(ProviderErrorCode::InvalidResponse);
                }
                for row in &rows {
                    if model::text(row, "instId")? != inst_id
                        || model::text(row, "mgnMode")? != margin_mode.as_okx()
                    {
                        return Err(ProviderErrorCode::InvalidResponse);
                    }
                    let pos_side = match order_validation::optional_text(row, "posSide")?.as_deref()
                    {
                        None | Some("") => None,
                        Some("net") => Some(model::PositionSide::Net),
                        Some("long") => Some(model::PositionSide::Long),
                        Some("short") => Some(model::PositionSide::Short),
                        Some(_) => return Err(ProviderErrorCode::InvalidResponse),
                    };
                    leverage.push(InstrumentLeverage {
                        inst_id: inst_id.to_string(),
                        margin_mode,
                        pos_side,
                        leverage: model::decimal(row, "lever")?,
                    });
                }
            }
        }
        Ok(AccountTradeContext {
            acct_lv,
            pos_mode,
            positions,
            pending_orders,
            leverage,
        })
    }

    fn read_pending_orders(
        &self,
        credentials: &OkxCredentials<'_>,
        inst_type: model::OrderInstrumentType,
        inst_id: &str,
        environment: OkxEnvironment,
    ) -> Result<Vec<model::PendingOrder>, ProviderErrorCode> {
        use std::collections::HashSet;

        let mut orders = Vec::new();
        let mut seen_order_ids = HashSet::new();
        let mut after: Option<String> = None;

        // OKX returns at most 100 rows per page and caps pending orders at 500
        // for one trading symbol. Both bounds keep this read finite.
        let probe_page = MAX_PENDING_ORDERS_PER_INSTRUMENT / PENDING_ORDERS_PAGE_SIZE;
        for page_number in 0..=probe_page {
            let mut params = vec![
                ("instType", inst_type.as_okx().to_string()),
                ("instId", inst_id.to_string()),
                ("limit", PENDING_ORDERS_PAGE_SIZE.to_string()),
            ];
            if let Some(cursor) = after.as_ref() {
                params.push(("after", cursor.clone()));
            }
            let rows: Vec<serde_json::Value> = self.get_with_environment(
                "/api/v5/trade/orders-pending",
                &params,
                Some(credentials),
                environment,
            )?;
            if rows.len() > PENDING_ORDERS_PAGE_SIZE {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            if page_number == probe_page {
                // Legacy accounts can exceed the published 500-order cap.
                // Only an empty probe proves that the capped result is complete.
                if rows.is_empty() {
                    break;
                }
                return Err(ProviderErrorCode::InvalidResponse);
            }
            if orders.len().saturating_add(rows.len()) > MAX_PENDING_ORDERS_PER_INSTRUMENT {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            if rows.is_empty() {
                break;
            }

            let mut page_order_ids = Vec::with_capacity(rows.len());
            for row in &rows {
                let order_id = model::id(row, "ordId")?;
                if !seen_order_ids.insert(order_id.clone()) {
                    // Duplicate IDs indicate a repeated/overlapping page. Do not
                    // use a partial list to authorize a close order.
                    return Err(ProviderErrorCode::InvalidResponse);
                }
                page_order_ids.push(order_id);
                orders.push(order_validation::parse_pending_order(row)?);
            }

            if rows.len() < PENDING_ORDERS_PAGE_SIZE {
                break;
            }

            let next_cursor = page_order_ids
                .last()
                .cloned()
                .ok_or(ProviderErrorCode::InvalidResponse)?;
            if after.as_ref() == Some(&next_cursor) {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            after = Some(next_cursor);
        }
        Ok(orders)
    }
}

fn parse_okx_envelope<T: serde::de::DeserializeOwned>(
    value: serde_json::Value,
) -> Result<T, ProviderErrorCode> {
    let code = value
        .get("code")
        .and_then(serde_json::Value::as_str)
        .ok_or(ProviderErrorCode::InvalidResponse)?;
    if code != "0" {
        return Err(map_okx_code(code));
    }
    serde_json::from_value(
        value
            .get("data")
            .cloned()
            .ok_or(ProviderErrorCode::InvalidResponse)?,
    )
    .map_err(|_| ProviderErrorCode::InvalidResponse)
}

fn hmac_base64(key: &[u8], message: &[u8]) -> Result<String, ProviderErrorCode> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| ProviderErrorCode::InvalidCredentials)?;
    mac.update(message);
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

fn validate_currency(currency: &str) -> Result<(), ProviderErrorCode> {
    if currency.is_empty()
        || currency.len() > 32
        || !currency.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(())
}

pub(super) fn map_okx_code(code: &str) -> ProviderErrorCode {
    match code {
        "50011" | "50040" => ProviderErrorCode::RateLimited,
        "50102" => ProviderErrorCode::InvalidRequest,
        "50100" | "50101" | "50111" | "50113" | "50119" | "60009" => {
            ProviderErrorCode::InvalidCredentials
        }
        _ => ProviderErrorCode::ProviderFailure,
    }
}

#[derive(Deserialize)]
struct OkxAccountBalance {
    details: Vec<OkxBalanceDetail>,
}

#[derive(Deserialize)]
struct OkxBalanceDetail {
    #[serde(rename = "ccy")]
    currency: String,
    #[serde(rename = "availBal")]
    available: String,
    #[serde(rename = "frozenBal")]
    frozen: String,
}
