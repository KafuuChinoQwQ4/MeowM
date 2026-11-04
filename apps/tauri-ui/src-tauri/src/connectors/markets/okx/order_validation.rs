use super::model::{
    AccountTradeContext, InstrumentSpec, OkxEnvironment, OrderDraft, OrderInstrumentType,
    OrderSide, OrderType, PendingOrder, PositionAction, PositionMode, PositionSide, TradeMode,
    TradePosition,
};
use crate::connectors::model::ProviderErrorCode;
use ledger_domain::crypto::{valid_asset, valid_decimal, valid_spot_instrument};
use serde::Serialize;

const DEFAULT_SPOT_SLIPPAGE: &str = "0.01";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct OrderBody {
    #[serde(rename = "instId")]
    inst_id: String,
    #[serde(rename = "tdMode")]
    td_mode: &'static str,
    #[serde(rename = "clOrdId")]
    cl_ord_id: String,
    side: OrderSide,
    #[serde(rename = "ordType")]
    ord_type: OrderType,
    sz: String,
    #[serde(skip_serializing_if = "Option::is_none", rename = "px")]
    price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tgtCcy")]
    target_currency: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "slippagePct")]
    slippage_pct: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "posSide")]
    position_side: Option<PositionSide>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "reduceOnly")]
    reduce_only: Option<bool>,
}

impl OrderBody {
    pub(crate) fn side(&self) -> OrderSide {
        self.side
    }

    pub(crate) fn tgt_ccy(&self) -> Option<&'static str> {
        self.target_currency
    }

    pub(crate) fn slippage_pct(&self) -> Option<&str> {
        self.slippage_pct.as_deref()
    }

    pub(crate) fn pos_side(&self) -> Option<PositionSide> {
        self.position_side
    }

    pub(crate) fn reduce_only(&self) -> Option<bool> {
        self.reduce_only
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValidatedOrder {
    environment: OkxEnvironment,
    body: OrderBody,
}

impl ValidatedOrder {
    pub(crate) fn from_draft(
        draft: OrderDraft,
        spec: &InstrumentSpec,
        context: &AccountTradeContext,
    ) -> Result<Self, ProviderErrorCode> {
        validate_spec(spec)?;
        if draft.inst_type != spec.inst_type || draft.inst_id != spec.inst_id {
            return Err(ProviderErrorCode::InvalidRequest);
        }
        if !valid_client_order_id(&draft.cl_ord_id) || !positive_decimal(&draft.size) {
            return Err(ProviderErrorCode::InvalidRequest);
        }

        let (td_mode, side, position_side, reduce_only) = match draft.inst_type {
            OrderInstrumentType::Spot => {
                if draft.position_action.is_some()
                    || draft.td_mode.is_some_and(|mode| mode != TradeMode::Cash)
                {
                    return Err(ProviderErrorCode::InvalidRequest);
                }
                (TradeMode::Cash.as_okx(), draft.side, None, None)
            }
            OrderInstrumentType::Swap | OrderInstrumentType::Futures => {
                let Some(td_mode @ (TradeMode::Cross | TradeMode::Isolated)) = draft.td_mode else {
                    return Err(ProviderErrorCode::InvalidRequest);
                };
                let Some(action) = draft.position_action else {
                    return Err(ProviderErrorCode::InvalidRequest);
                };
                let (side, position_side, reduce_only) =
                    contract_order_mapping(action, context.pos_mode);
                if draft.side != side {
                    return Err(ProviderErrorCode::InvalidRequest);
                }
                if is_close(action) {
                    validate_close_size(&draft, spec, context, side, position_side)?;
                }
                (td_mode.as_okx(), side, position_side, reduce_only)
            }
        };

        let size_uses_base_or_contract_units = !matches!(
            (draft.inst_type, draft.ord_type, draft.side),
            (
                OrderInstrumentType::Spot,
                OrderType::Market,
                OrderSide::Sell
            )
        );
        if size_uses_base_or_contract_units
            && (!is_multiple(&draft.size, &spec.lot_sz)
                || compare_decimal(&draft.size, &spec.min_sz)
                    .is_none_or(|ordering| ordering.is_lt()))
        {
            return Err(ProviderErrorCode::InvalidRequest);
        }

        let target_currency;
        let slippage_pct;
        match (draft.inst_type, draft.ord_type) {
            (OrderInstrumentType::Spot, OrderType::Market) => {
                target_currency = Some(match draft.side {
                    OrderSide::Buy => "base_ccy",
                    OrderSide::Sell => "quote_ccy",
                });
                let slippage = draft
                    .slippage_pct
                    .as_deref()
                    .unwrap_or(DEFAULT_SPOT_SLIPPAGE);
                validate_slippage(slippage)?;
                slippage_pct = Some(slippage.to_string());
            }
            (OrderInstrumentType::Spot, OrderType::Limit) => {
                if draft.slippage_pct.is_some() {
                    return Err(ProviderErrorCode::InvalidRequest);
                }
                target_currency = None;
                slippage_pct = None;
            }
            (OrderInstrumentType::Swap | OrderInstrumentType::Futures, _) => {
                if draft.slippage_pct.is_some() {
                    return Err(ProviderErrorCode::InvalidRequest);
                }
                target_currency = None;
                slippage_pct = None;
            }
        }

        let price = match (draft.ord_type, draft.price) {
            (OrderType::Limit, Some(price))
                if positive_decimal(&price) && is_multiple(&price, &spec.tick_sz) =>
            {
                Some(price)
            }
            (OrderType::Limit, _) => return Err(ProviderErrorCode::InvalidRequest),
            (OrderType::Market, None) => None,
            (OrderType::Market, Some(_)) => return Err(ProviderErrorCode::InvalidRequest),
        };

        Ok(Self {
            environment: draft.environment,
            body: OrderBody {
                inst_id: draft.inst_id,
                td_mode,
                cl_ord_id: draft.cl_ord_id,
                side,
                ord_type: draft.ord_type,
                sz: draft.size,
                price,
                target_currency,
                slippage_pct,
                position_side,
                reduce_only,
            },
        })
    }

    pub(crate) fn environment(&self) -> OkxEnvironment {
        self.environment
    }

    pub(crate) fn body(&self) -> &OrderBody {
        &self.body
    }
}

fn validate_spec(spec: &InstrumentSpec) -> Result<(), ProviderErrorCode> {
    validate_instrument_metadata(spec)?;
    if spec.state != "live" {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(())
}

fn validate_instrument_metadata(spec: &InstrumentSpec) -> Result<(), ProviderErrorCode> {
    if !valid_instrument_id(spec.inst_type, &spec.inst_id)
        || !positive_decimal(&spec.tick_sz)
        || !positive_decimal(&spec.lot_sz)
        || !positive_decimal(&spec.min_sz)
    {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    match spec.inst_type {
        OrderInstrumentType::Spot => {
            if !spec.base_ccy.as_deref().is_some_and(valid_asset)
                || !spec.quote_ccy.as_deref().is_some_and(valid_asset)
                || spec.base_ccy == spec.quote_ccy
            {
                return Err(ProviderErrorCode::InvalidRequest);
            }
        }
        OrderInstrumentType::Swap | OrderInstrumentType::Futures => {
            if !spec.ct_val.as_deref().is_some_and(positive_decimal)
                || !spec.ct_val_ccy.as_deref().is_some_and(valid_currency)
                || !spec.settle_ccy.as_deref().is_some_and(valid_currency)
                || !spec
                    .ct_type
                    .as_deref()
                    .is_some_and(|kind| kind == "linear" || kind == "inverse")
            {
                return Err(ProviderErrorCode::InvalidRequest);
            }
        }
    }
    Ok(())
}

pub(super) fn valid_instrument_id(inst_type: OrderInstrumentType, inst_id: &str) -> bool {
    match inst_type {
        OrderInstrumentType::Spot => valid_spot_instrument(inst_id),
        OrderInstrumentType::Swap => {
            let parts: Vec<_> = inst_id.split('-').collect();
            parts.len() == 3
                && parts[2] == "SWAP"
                && parts[..2].iter().all(|part| valid_contract_token(part))
        }
        OrderInstrumentType::Futures => {
            let parts: Vec<_> = inst_id.split('-').collect();
            parts.len() == 3
                && parts[..2].iter().all(|part| valid_contract_token(part))
                && parts[2].len() == 6
                && parts[2].bytes().all(|byte| byte.is_ascii_digit())
        }
    }
}

pub(super) fn parse_instrument_spec(
    inst_type: OrderInstrumentType,
    row: &serde_json::Value,
) -> Result<InstrumentSpec, ProviderErrorCode> {
    if super::model::text(row, "instType")? != inst_type.as_okx() {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    let spec = InstrumentSpec {
        inst_type,
        inst_id: super::model::text(row, "instId")?,
        state: super::model::text(row, "state")?,
        tick_sz: super::model::decimal(row, "tickSz")?,
        lot_sz: super::model::decimal(row, "lotSz")?,
        min_sz: super::model::decimal(row, "minSz")?,
        ct_val: super::model::optional_decimal(row, "ctVal")?,
        ct_val_ccy: optional_asset(row, "ctValCcy")?,
        ct_type: optional_text(row, "ctType")?,
        settle_ccy: optional_asset(row, "settleCcy")?,
        base_ccy: optional_asset(row, "baseCcy")?,
        quote_ccy: optional_asset(row, "quoteCcy")?,
    };
    validate_instrument_metadata(&spec).map_err(|_| ProviderErrorCode::InvalidResponse)?;
    Ok(spec)
}

pub(super) fn parse_position(row: &serde_json::Value) -> Result<TradePosition, ProviderErrorCode> {
    let pos_side = parse_position_side(&super::model::text(row, "posSide")?)?;
    let margin_mode = parse_trade_mode(&super::model::text(row, "mgnMode")?)?;
    let quantity = super::model::decimal(row, "pos")?;
    if !valid_instrument_id_from_string(&super::model::text(row, "instId")?) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    Ok(TradePosition {
        inst_id: super::model::text(row, "instId")?,
        pos_side,
        quantity,
        margin_mode,
    })
}

pub(super) fn parse_pending_order(
    row: &serde_json::Value,
) -> Result<PendingOrder, ProviderErrorCode> {
    let inst_id = super::model::text(row, "instId")?;
    if !valid_instrument_id_from_string(&inst_id) {
        return Err(ProviderErrorCode::InvalidResponse);
    }
    let side = match super::model::text(row, "side")?.as_str() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return Err(ProviderErrorCode::InvalidResponse),
    };
    let pos_side = match optional_text(row, "posSide")?.as_deref() {
        None | Some("") => None,
        Some(value) => Some(parse_position_side(value)?),
    };
    let margin_mode = parse_trade_mode(&super::model::text(row, "tdMode")?)?;
    let reduce_only = match optional_text(row, "reduceOnly")?.as_deref() {
        Some("true") => true,
        Some("false") | None | Some("") => false,
        _ => return Err(ProviderErrorCode::InvalidResponse),
    };
    let size = super::model::decimal(row, "sz")?;
    let filled = super::model::decimal(row, "accFillSz")?;
    let remaining_size = subtract_decimal(&size, &filled)
        .filter(|value| !value.starts_with('-'))
        .ok_or(ProviderErrorCode::InvalidResponse)?;
    Ok(PendingOrder {
        inst_id,
        side,
        pos_side,
        margin_mode,
        reduce_only,
        remaining_size,
    })
}

fn valid_instrument_id_from_string(inst_id: &str) -> bool {
    let parts: Vec<_> = inst_id.split('-').collect();
    if parts.len() == 2 {
        valid_spot_instrument(inst_id)
    } else if parts.len() == 3 && parts[2] == "SWAP" {
        valid_instrument_id(OrderInstrumentType::Swap, inst_id)
    } else {
        valid_instrument_id(OrderInstrumentType::Futures, inst_id)
    }
}

fn valid_contract_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

fn valid_currency(value: &str) -> bool {
    valid_asset(value) || value == "USDⓈ"
}

pub(super) fn optional_text(
    row: &serde_json::Value,
    key: &str,
) -> Result<Option<String>, ProviderErrorCode> {
    match row.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(value)) if value.is_empty() => Ok(None),
        _ => super::model::text(row, key).map(Some),
    }
}

fn optional_asset(row: &serde_json::Value, key: &str) -> Result<Option<String>, ProviderErrorCode> {
    match optional_text(row, key)? {
        Some(value) if valid_currency(&value) => Ok(Some(value)),
        Some(_) => Err(ProviderErrorCode::InvalidResponse),
        None => Ok(None),
    }
}

fn parse_position_side(value: &str) -> Result<PositionSide, ProviderErrorCode> {
    match value {
        "net" => Ok(PositionSide::Net),
        "long" => Ok(PositionSide::Long),
        "short" => Ok(PositionSide::Short),
        _ => Err(ProviderErrorCode::InvalidResponse),
    }
}

fn parse_trade_mode(value: &str) -> Result<TradeMode, ProviderErrorCode> {
    match value {
        "cash" => Ok(TradeMode::Cash),
        "cross" => Ok(TradeMode::Cross),
        "isolated" => Ok(TradeMode::Isolated),
        _ => Err(ProviderErrorCode::InvalidResponse),
    }
}

fn subtract_decimal(left: &str, right: &str) -> Option<String> {
    let left = parse_decimal(left)?;
    let right = parse_decimal(right)?;
    let result = left.checked_sub(right)?;
    Some(decimal_string(result))
}

fn decimal_string(decimal: Decimal) -> String {
    let sign = if decimal.coefficient < 0 { "-" } else { "" };
    let mut digits = decimal.coefficient.unsigned_abs().to_string();
    let scale = decimal.scale as usize;
    if scale > 0 {
        if digits.len() <= scale {
            digits = format!("{}{}", "0".repeat(scale + 1 - digits.len()), digits);
        }
        digits.insert(digits.len() - scale, '.');
        digits = digits
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
    }
    format!("{sign}{digits}")
}

fn valid_client_order_id(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn positive_decimal(value: &str) -> bool {
    parse_decimal(value).is_some_and(|decimal| decimal.coefficient > 0)
}

fn validate_slippage(value: &str) -> Result<(), ProviderErrorCode> {
    let Some(decimal) = parse_decimal(value) else {
        return Err(ProviderErrorCode::InvalidRequest);
    };
    let fraction_digits = value
        .split_once('.')
        .map_or(0, |(_, fraction)| fraction.len());
    if value.starts_with('-')
        || decimal.coefficient < 0
        || fraction_digits > 4
        || compare_decimal(value, "0.05").is_none_or(|ordering| ordering.is_gt())
    {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(())
}

fn contract_order_mapping(
    action: PositionAction,
    mode: PositionMode,
) -> (OrderSide, Option<PositionSide>, Option<bool>) {
    match (mode, action) {
        (PositionMode::Net, PositionAction::OpenLong) => (OrderSide::Buy, None, None),
        (PositionMode::Net, PositionAction::OpenShort) => (OrderSide::Sell, None, None),
        (PositionMode::Net, PositionAction::CloseLong) => (OrderSide::Sell, None, Some(true)),
        (PositionMode::Net, PositionAction::CloseShort) => (OrderSide::Buy, None, Some(true)),
        (PositionMode::LongShort, PositionAction::OpenLong) => {
            (OrderSide::Buy, Some(PositionSide::Long), None)
        }
        (PositionMode::LongShort, PositionAction::OpenShort) => {
            (OrderSide::Sell, Some(PositionSide::Short), None)
        }
        (PositionMode::LongShort, PositionAction::CloseLong) => {
            (OrderSide::Sell, Some(PositionSide::Long), None)
        }
        (PositionMode::LongShort, PositionAction::CloseShort) => {
            (OrderSide::Buy, Some(PositionSide::Short), None)
        }
    }
}

fn is_close(action: PositionAction) -> bool {
    matches!(
        action,
        PositionAction::CloseLong | PositionAction::CloseShort
    )
}

fn validate_close_size(
    draft: &OrderDraft,
    spec: &InstrumentSpec,
    context: &AccountTradeContext,
    close_side: OrderSide,
    position_side: Option<PositionSide>,
) -> Result<(), ProviderErrorCode> {
    let td_mode = draft.td_mode.ok_or(ProviderErrorCode::InvalidRequest)?;
    let close_long = draft.position_action == Some(PositionAction::CloseLong);
    let expected_side = if close_long {
        PositionSide::Long
    } else {
        PositionSide::Short
    };

    let mut position_size = Decimal::zero();
    for position in &context.positions {
        if position.inst_id != spec.inst_id || position.margin_mode != td_mode {
            continue;
        }
        match context.pos_mode {
            PositionMode::Net if position.pos_side == PositionSide::Net => {
                let Some(quantity) = parse_decimal(&position.quantity) else {
                    return Err(ProviderErrorCode::InvalidRequest);
                };
                if (close_long && quantity.coefficient > 0)
                    || (!close_long && quantity.coefficient < 0)
                {
                    position_size = position_size
                        .checked_add(quantity.abs())
                        .ok_or(ProviderErrorCode::InvalidRequest)?;
                }
            }
            PositionMode::LongShort if position.pos_side == expected_side => {
                let Some(quantity) = parse_decimal(&position.quantity) else {
                    return Err(ProviderErrorCode::InvalidRequest);
                };
                if quantity.coefficient <= 0 {
                    return Err(ProviderErrorCode::InvalidRequest);
                }
                position_size = position_size
                    .checked_add(quantity)
                    .ok_or(ProviderErrorCode::InvalidRequest)?;
            }
            _ => {}
        }
    }

    let mut pending_size = Decimal::zero();
    for pending in &context.pending_orders {
        let closes_matching_position = match context.pos_mode {
            PositionMode::Net => {
                pending.reduce_only
                    && pending.side == close_side
                    && pending
                        .pos_side
                        .is_none_or(|side| side == PositionSide::Net)
            }
            PositionMode::LongShort => {
                pending.side == close_side && pending.pos_side == position_side
            }
        };
        if pending.inst_id == spec.inst_id
            && pending.margin_mode == td_mode
            && closes_matching_position
        {
            let Some(size) = parse_decimal(&pending.remaining_size) else {
                return Err(ProviderErrorCode::InvalidRequest);
            };
            if size.coefficient < 0 {
                return Err(ProviderErrorCode::InvalidRequest);
            }
            pending_size = pending_size
                .checked_add(size)
                .ok_or(ProviderErrorCode::InvalidRequest)?;
        }
    }

    let available = position_size
        .checked_sub(pending_size)
        .unwrap_or_else(Decimal::zero);
    let requested = parse_decimal(&draft.size).ok_or(ProviderErrorCode::InvalidRequest)?;
    if available
        .compare(requested)
        .is_none_or(|ordering| ordering.is_lt())
    {
        return Err(ProviderErrorCode::InvalidRequest);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Decimal {
    coefficient: i128,
    scale: u32,
}

impl Decimal {
    fn zero() -> Self {
        Self {
            coefficient: 0,
            scale: 0,
        }
    }

    fn abs(self) -> Self {
        Self {
            coefficient: self.coefficient.abs(),
            scale: self.scale,
        }
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        let (left, right, scale) = self.align(other)?;
        Some(Self {
            coefficient: left.checked_add(right)?,
            scale,
        })
    }

    fn checked_sub(self, other: Self) -> Option<Self> {
        let (left, right, scale) = self.align(other)?;
        Some(Self {
            coefficient: left.checked_sub(right)?,
            scale,
        })
    }

    fn compare(self, other: Self) -> Option<std::cmp::Ordering> {
        let (left, right, _) = self.align(other)?;
        Some(left.cmp(&right))
    }

    fn align(self, other: Self) -> Option<(i128, i128, u32)> {
        let scale = self.scale.max(other.scale);
        Some((
            scale_decimal(self.coefficient, scale - self.scale)?,
            scale_decimal(other.coefficient, scale - other.scale)?,
            scale,
        ))
    }
}

fn parse_decimal(value: &str) -> Option<Decimal> {
    if !valid_decimal(value) {
        return None;
    }
    let negative = value.starts_with('-');
    let value = value.strip_prefix('-').unwrap_or(value);
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let fraction = fraction.trim_end_matches('0');
    let mut coefficient = format!("{whole}{fraction}").parse::<i128>().ok()?;
    if negative {
        coefficient = coefficient.checked_neg()?;
    }
    Some(Decimal {
        coefficient,
        scale: fraction.len() as u32,
    })
}

fn compare_decimal(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    parse_decimal(left)?.compare(parse_decimal(right)?)
}

fn is_multiple(value: &str, step: &str) -> bool {
    let (Some(value), Some(step)) = (parse_decimal(value), parse_decimal(step)) else {
        return false;
    };
    if step.coefficient <= 0 {
        return false;
    }
    let Some((value, step, _)) = value.align(step) else {
        return false;
    };
    value % step == 0
}

fn scale_decimal(value: i128, places: u32) -> Option<i128> {
    value.checked_mul(10_i128.checked_pow(places)?)
}
