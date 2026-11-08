//! Decode the OKX SBE market-data schema 1, version 0 (semantic version 1.0.0).
//!
//! This decodes a single event only. Maintaining or synchronizing an order book
//! requires processing snapshots, incremental events, and exponent updates in
//! sequence; this module intentionally does not create an order book.

use serde_json::{Value, json};

const SCHEMA_ID: u16 = 1;
const SCHEMA_VERSION: u16 = 0;
const LEVEL_BLOCK_LENGTH: usize = 20;

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], &'static str> {
        let end = self.offset.checked_add(count).ok_or("invalid length")?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or("truncated SBE message")?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, &'static str> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn i8(&mut self) -> Result<i8, &'static str> {
        Ok(self.take(1)?[0] as i8)
    }

    fn i16(&mut self) -> Result<i16, &'static str> {
        let bytes = self.take(2)?;
        Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn i32(&mut self) -> Result<i32, &'static str> {
        let bytes = self.take(4)?;
        Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i64(&mut self) -> Result<i64, &'static str> {
        let bytes = self.take(8)?;
        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }
}

/// Convert mantissa × 10^exponent without floating-point rounding.
fn decimal(mantissa: i64, exponent: i8) -> String {
    let digits = mantissa.unsigned_abs().to_string();
    let sign = if mantissa < 0 { "-" } else { "" };
    if exponent >= 0 {
        return format!("{sign}{digits}{}", "0".repeat(exponent as usize));
    }
    let places = (-(exponent as i16)) as usize;
    if places < digits.len() {
        let split = digits.len() - places;
        format!("{sign}{}.{}", &digits[..split], &digits[split..])
    } else {
        format!("{sign}0.{}{digits}", "0".repeat(places - digits.len()))
    }
}

fn levels(reader: &mut Reader<'_>, px_exp: i8, sz_exp: i8) -> Result<Vec<Value>, &'static str> {
    let block_length = usize::from(reader.u16()?);
    let count = usize::from(reader.u16()?);
    if block_length < LEVEL_BLOCK_LENGTH {
        return Err("unsupported SBE group block length");
    }
    let total = block_length
        .checked_mul(count)
        .ok_or("invalid SBE group length")?;
    if total > reader.remaining() {
        return Err("truncated SBE group");
    }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let mut row = Reader::new(reader.take(block_length)?);
        let px = row.i64()?;
        let sz = row.i64()?;
        let ord_count = row.i32()?;
        result.push(json!({
            "pxMantissa": px.to_string(), "szMantissa": sz.to_string(), "ordCount": ord_count,
            "px": decimal(px, px_exp), "sz": decimal(sz, sz_exp),
        }));
    }
    Ok(result)
}

/// Decode one complete binary event from the published OKX SBE XML schema.
/// Unknown schema versions, templates, enum values, malformed groups, and
/// incomplete or trailing bytes are rejected rather than guessed.
pub(crate) fn decode_sbe(bytes: &[u8]) -> Result<Value, &'static str> {
    let mut reader = Reader::new(bytes);
    let block_length = reader.u16()?;
    let template_id = reader.u16()?;
    let schema_id = reader.u16()?;
    let version = reader.u16()?;
    if schema_id != SCHEMA_ID || version != SCHEMA_VERSION {
        return Err("unsupported SBE schema or version");
    }
    let name = match template_id {
        1000 => "BboTbtChannelEvent",
        1001 => "BooksL2TbtChannelEvent",
        1002 => "BooksL2TbtExponentUpdateEvent",
        1003 => "BooksL2TbtElpChannelEvent",
        1004 => "BooksL2TbtElpExponentUpdateEvent",
        1005 => "TradesChannelEvent",
        1006 => "SnapshotDepthResponseEvent",
        _ => return Err("unsupported SBE template"),
    };
    // The header's acting block length may be greater than the fields known to
    // this decoder. Slice the complete block so any appended fixed fields are
    // skipped before reading the first repeating-group dimension.
    let mut root = Reader::new(reader.take(usize::from(block_length))?);
    let mut result = match template_id {
        1000 => {
            let inst = root.i64()?;
            let ts = root.i64()?;
            let out = root.i64()?;
            let seq = root.i64()?;
            let ask_px = root.i64()?;
            let ask_sz = root.i64()?;
            let bid_px = root.i64()?;
            let bid_sz = root.i64()?;
            let ask_count = root.i32()?;
            let bid_count = root.i32()?;
            let px_exp = root.i8()?;
            let sz_exp = root.i8()?;
            json!({
                "instIdCode": inst.to_string(), "tsUs": ts.to_string(), "outTime": out.to_string(),
                "seqId": seq.to_string(), "askPxMantissa": ask_px.to_string(),
                "askSzMantissa": ask_sz.to_string(), "bidPxMantissa": bid_px.to_string(),
                "bidSzMantissa": bid_sz.to_string(), "askOrdCount": ask_count, "bidOrdCount": bid_count,
                "pxExponent": px_exp, "szExponent": sz_exp,
                "askPx": decimal(ask_px, px_exp), "askSz": decimal(ask_sz, sz_exp),
                "bidPx": decimal(bid_px, px_exp), "bidSz": decimal(bid_sz, sz_exp),
            })
        }
        1001..=1004 => {
            let inst = root.i64()?;
            let ts = root.i64()?;
            let out = root.i64()?;
            let seq = root.i64()?;
            let prev = root.i64()?;
            let px_exp = root.i8()?;
            let sz_exp = root.i8()?;
            let mut event = json!({
                "instIdCode": inst.to_string(), "tsUs": ts.to_string(), "outTime": out.to_string(),
                "seqId": seq.to_string(), "prevSeqId": prev.to_string(),
                "pxExponent": px_exp, "szExponent": sz_exp,
            });
            if template_id == 1001 || template_id == 1003 {
                event["asks"] = levels(&mut reader, px_exp, sz_exp)?.into();
                event["bids"] = levels(&mut reader, px_exp, sz_exp)?.into();
            }
            event
        }
        1005 => {
            let inst = root.i64()?;
            let ts = root.i64()?;
            let out = root.i64()?;
            let seq = root.i64()?;
            let px = root.i64()?;
            let sz = root.i64()?;
            let trade_id = root.i64()?;
            let count = root.i16()?;
            let side = match root.i8()? {
                0 => "SELL",
                1 => "BUY",
                _ => return Err("unsupported SBE trade side"),
            };
            let px_exp = root.i8()?;
            let sz_exp = root.i8()?;
            let source = match root.i8()? {
                0 => "NORMAL",
                1 => "ELP",
                _ => return Err("unsupported SBE trade source"),
            };
            json!({
                "instIdCode": inst.to_string(), "tsUs": ts.to_string(), "outTime": out.to_string(),
                "seqId": seq.to_string(), "pxMantissa": px.to_string(), "szMantissa": sz.to_string(),
                "tradeId": trade_id.to_string(), "count": count, "side": side,
                "pxExponent": px_exp, "szExponent": sz_exp, "source": source,
                "px": decimal(px, px_exp), "sz": decimal(sz, sz_exp),
            })
        }
        1006 => {
            let inst = root.i64()?;
            let ts = root.i64()?;
            let seq = root.i64()?;
            let px_exp = root.i8()?;
            let sz_exp = root.i8()?;
            json!({
                "instIdCode": inst.to_string(), "tsUs": ts.to_string(), "seqId": seq.to_string(),
                "pxExponent": px_exp, "szExponent": sz_exp,
                "asks": levels(&mut reader, px_exp, sz_exp)?, "bids": levels(&mut reader, px_exp, sz_exp)?,
            })
        }
        _ => unreachable!(),
    };
    if reader.remaining() != 0 {
        return Err("unexpected trailing SBE data");
    }
    result["event"] = name.into();
    result["templateId"] = template_id.into();
    result["schemaId"] = schema_id.into();
    result["version"] = version.into();
    result["blockLength"] = block_length.into();
    Ok(result)
}

#[cfg(test)]
#[path = "sbe_tests.rs"]
mod tests;
