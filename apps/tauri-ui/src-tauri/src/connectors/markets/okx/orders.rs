use super::model::{
    AccountTradeContext, InstrumentSpec, OkxEnvironment, OrderAcknowledgement, OrderDraft,
    OrderInstrumentType, OrderSnapshot,
};
use super::{OkxClient, OkxCredentials, OrderPostFailure, order_validation};
use crate::connectors::{
    credential_store::CredentialService,
    model::{CredentialInput, ProviderErrorCode, ProviderId},
};
use ledger_domain::crypto::valid_external_id;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Serialize)]
pub(crate) struct TradeContextResponse {
    pub instrument: InstrumentSpec,
    pub account: AccountTradeContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OrderSubmissionStatus {
    Accepted,
    Rejected,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct OrderSubmissionResult {
    pub status: OrderSubmissionStatus,
    pub client_order_id: String,
    pub acknowledgement: Option<OrderAcknowledgement>,
    pub error_code: Option<ProviderErrorCode>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct OrderActionResult {
    pub status: OrderSubmissionStatus,
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub error_code: Option<ProviderErrorCode>,
}

#[derive(Serialize)]
struct CancelOrderBody<'a> {
    #[serde(rename = "instId")]
    inst_id: &'a str,
    #[serde(rename = "ordId")]
    order_id: &'a str,
}

#[derive(Deserialize)]
struct CancelAcknowledgement {
    #[serde(rename = "instId")]
    inst_id: String,
    #[serde(rename = "ordId")]
    order_id: String,
    #[serde(rename = "clOrdId")]
    client_order_id: String,
    #[serde(rename = "sCode")]
    code: String,
    #[serde(rename = "sMsg")]
    message: String,
}

pub(crate) struct OkxOrderService {
    client: OkxClient,
    credentials: Arc<CredentialService>,
}

impl OkxOrderService {
    pub(crate) fn new(client: OkxClient, credentials: Arc<CredentialService>) -> Self {
        Self {
            client,
            credentials,
        }
    }

    pub(crate) fn trade_context(
        &self,
        inst_type: OrderInstrumentType,
        inst_id: &str,
        environment: OkxEnvironment,
    ) -> Result<TradeContextResponse, ProviderErrorCode> {
        if !order_validation::valid_instrument_id(inst_type, inst_id) {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        with_trade_credentials(&self.credentials, environment, |credentials| {
            let instrument = self.client.read_instrument_spec(inst_type, inst_id)?;
            let account =
                self.client
                    .read_trade_context(credentials, inst_type, inst_id, environment)?;
            Ok(TradeContextResponse {
                instrument,
                account,
            })
        })
    }

    pub(crate) fn place_order(
        &self,
        draft: OrderDraft,
    ) -> Result<OrderSubmissionResult, ProviderErrorCode> {
        order_validation::validate_order_draft_shape(&draft)?;
        let environment = draft.environment;
        let inst_type = draft.inst_type;
        let inst_id = draft.inst_id.clone();
        let client_order_id = draft.cl_ord_id.clone();
        with_trade_credentials(&self.credentials, environment, |credentials| {
            let instrument = self.client.read_instrument_spec(inst_type, &inst_id)?;
            let account =
                self.client
                    .read_trade_context(credentials, inst_type, &inst_id, environment)?;
            let order = order_validation::ValidatedOrder::from_draft(draft, &instrument, &account)?;
            match self
                .client
                .post_with_outcome::<Vec<OrderAcknowledgement>, _>(
                    "/api/v5/trade/order",
                    order.body(),
                    credentials,
                    order.environment(),
                ) {
                Ok(mut acknowledgements) if acknowledgements.len() == 1 => {
                    let acknowledgement = acknowledgements.remove(0);
                    if acknowledgement.client_order_id != client_order_id
                        || (acknowledgement.code == "0"
                            && !valid_external_id(&acknowledgement.order_id))
                    {
                        return Ok(OrderSubmissionResult {
                            status: OrderSubmissionStatus::Unknown,
                            client_order_id,
                            acknowledgement: None,
                            error_code: Some(ProviderErrorCode::InvalidResponse),
                        });
                    }
                    Ok(OrderSubmissionResult {
                        status: if acknowledgement.code == "0" {
                            OrderSubmissionStatus::Accepted
                        } else {
                            OrderSubmissionStatus::Rejected
                        },
                        client_order_id,
                        acknowledgement: Some(acknowledgement),
                        error_code: None,
                    })
                }
                Ok(_) => Ok(OrderSubmissionResult {
                    status: OrderSubmissionStatus::Unknown,
                    client_order_id,
                    acknowledgement: None,
                    error_code: Some(ProviderErrorCode::InvalidResponse),
                }),
                Err(OrderPostFailure::Unconfirmed(error_code)) => Ok(OrderSubmissionResult {
                    status: OrderSubmissionStatus::Unknown,
                    client_order_id,
                    acknowledgement: None,
                    error_code: Some(error_code),
                }),
                Err(
                    OrderPostFailure::BeforeSend(error_code)
                    | OrderPostFailure::Rejected(error_code),
                ) => Err(error_code),
            }
        })
    }

    pub(crate) fn order_detail(
        &self,
        inst_id: &str,
        client_order_id: &str,
        environment: OkxEnvironment,
    ) -> Result<Option<OrderSnapshot>, ProviderErrorCode> {
        if !order_validation::valid_instrument_id_from_string(inst_id)
            || !order_validation::valid_client_order_id(client_order_id)
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        with_trade_credentials(&self.credentials, environment, |credentials| {
            let params = [
                ("instId", inst_id.to_string()),
                ("clOrdId", client_order_id.to_string()),
            ];
            let mut rows: Vec<OrderSnapshot> = self.client.get_with_environment(
                "/api/v5/trade/order",
                &params,
                Some(credentials),
                environment,
            )?;
            if rows.is_empty() {
                return Ok(None);
            }
            if rows.len() != 1
                || rows[0].inst_id != inst_id
                || rows[0].client_order_id != client_order_id
                || !valid_order_snapshot(&rows[0])
            {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            Ok(rows.pop())
        })
    }

    pub(crate) fn order_history(
        &self,
        inst_type: OrderInstrumentType,
        environment: OkxEnvironment,
    ) -> Result<Vec<OrderSnapshot>, ProviderErrorCode> {
        with_trade_credentials(&self.credentials, environment, |credentials| {
            let params = [
                ("instType", inst_type.as_okx().to_string()),
                ("limit", "100".to_string()),
            ];
            let rows: Vec<OrderSnapshot> = self.client.get_with_environment(
                "/api/v5/trade/orders-history",
                &params,
                Some(credentials),
                environment,
            )?;
            if rows.len() > 100
                || rows
                    .iter()
                    .any(|row| row.inst_type != inst_type.as_okx() || !valid_order_snapshot(row))
            {
                return Err(ProviderErrorCode::InvalidResponse);
            }
            Ok(rows)
        })
    }

    pub(crate) fn cancel_order(
        &self,
        inst_id: &str,
        order_id: &str,
        environment: OkxEnvironment,
    ) -> Result<OrderActionResult, ProviderErrorCode> {
        if !order_validation::valid_instrument_id_from_string(inst_id)
            || !valid_external_id(order_id)
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        with_trade_credentials(&self.credentials, environment, |credentials| {
            let body = CancelOrderBody { inst_id, order_id };
            match self
                .client
                .post_with_outcome::<Vec<CancelAcknowledgement>, _>(
                    "/api/v5/trade/cancel-order",
                    &body,
                    credentials,
                    environment,
                ) {
                Ok(mut acknowledgements) if acknowledgements.len() == 1 => {
                    let acknowledgement = acknowledgements.remove(0);
                    if acknowledgement.inst_id != inst_id || acknowledgement.order_id != order_id {
                        return Ok(OrderActionResult {
                            status: OrderSubmissionStatus::Unknown,
                            order_id: order_id.to_string(),
                            client_order_id: None,
                            code: None,
                            message: None,
                            error_code: Some(ProviderErrorCode::InvalidResponse),
                        });
                    }
                    Ok(OrderActionResult {
                        status: if acknowledgement.code == "0" {
                            OrderSubmissionStatus::Accepted
                        } else {
                            OrderSubmissionStatus::Rejected
                        },
                        order_id: order_id.to_string(),
                        client_order_id: (!acknowledgement.client_order_id.is_empty())
                            .then_some(acknowledgement.client_order_id),
                        code: Some(acknowledgement.code),
                        message: Some(acknowledgement.message),
                        error_code: None,
                    })
                }
                Ok(_) => Ok(OrderActionResult {
                    status: OrderSubmissionStatus::Unknown,
                    order_id: order_id.to_string(),
                    client_order_id: None,
                    code: None,
                    message: None,
                    error_code: Some(ProviderErrorCode::InvalidResponse),
                }),
                Err(OrderPostFailure::Unconfirmed(error_code)) => Ok(OrderActionResult {
                    status: OrderSubmissionStatus::Unknown,
                    order_id: order_id.to_string(),
                    client_order_id: None,
                    code: None,
                    message: None,
                    error_code: Some(error_code),
                }),
                Err(
                    OrderPostFailure::BeforeSend(error_code)
                    | OrderPostFailure::Rejected(error_code),
                ) => Err(error_code),
            }
        })
    }
}

fn with_trade_credentials<T>(
    service: &CredentialService,
    environment: OkxEnvironment,
    operation: impl FnOnce(&OkxCredentials<'_>) -> Result<T, ProviderErrorCode>,
) -> Result<T, ProviderErrorCode> {
    let provider = match environment {
        OkxEnvironment::Demo => ProviderId::OkxTradeDemo,
        OkxEnvironment::Production => ProviderId::OkxTradeLive,
    };
    let loaded = service
        .load(provider)
        .map_err(|_| ProviderErrorCode::SecureStorageUnavailable)?;
    let Some(loaded) = loaded else {
        return Err(ProviderErrorCode::InvalidCredentials);
    };
    match (environment, loaded) {
        (
            OkxEnvironment::Demo,
            CredentialInput::OkxTradeDemo {
                api_key,
                secret_key,
                passphrase,
            },
        )
        | (
            OkxEnvironment::Production,
            CredentialInput::OkxTradeLive {
                api_key,
                secret_key,
                passphrase,
            },
        ) => operation(&OkxCredentials {
            api_key: api_key.expose_secret(),
            secret_key: secret_key.expose_secret(),
            passphrase: passphrase.expose_secret(),
        }),
        _ => Err(ProviderErrorCode::InvalidCredentials),
    }
}

fn valid_order_snapshot(row: &OrderSnapshot) -> bool {
    let instrument_type = match row.inst_type.as_str() {
        "SPOT" => OrderInstrumentType::Spot,
        "SWAP" => OrderInstrumentType::Swap,
        "FUTURES" => OrderInstrumentType::Futures,
        _ => return false,
    };
    order_validation::valid_instrument_id(instrument_type, &row.inst_id)
        && valid_external_id(&row.order_id)
        && matches!(row.side.as_str(), "buy" | "sell")
        && matches!(
            row.order_type.as_str(),
            "market" | "limit" | "post_only" | "fok" | "ioc" | "optimal_limit_ioc" | "rpi"
        )
        && matches!(row.trade_mode.as_str(), "cash" | "cross" | "isolated")
}
