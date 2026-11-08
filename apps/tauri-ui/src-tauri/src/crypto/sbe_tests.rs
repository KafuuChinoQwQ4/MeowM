use super::decode_sbe;
use serde_json::json;

fn header(template: u16, block_length: u16) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend(block_length.to_le_bytes());
    frame.extend(template.to_le_bytes());
    frame.extend(1u16.to_le_bytes()); // OKX schema ID
    frame.extend(0u16.to_le_bytes()); // XML schema version
    frame
}

fn push_i64(frame: &mut Vec<u8>, value: i64) {
    frame.extend(value.to_le_bytes());
}

fn bbo() -> Vec<u8> {
    let mut frame = header(1000, 74);
    for value in [
        211_874,
        1_706_233_600_001_000,
        1_706_233_600_002_000,
        9_007_199_254_740_993,
        123_456,
        1_005,
        123_400,
        25,
    ] {
        push_i64(&mut frame, value);
    }
    frame.extend(2i32.to_le_bytes());
    frame.extend(3i32.to_le_bytes());
    frame.extend([(-2i8) as u8, (-3i8) as u8]);
    frame
}

fn append_group(frame: &mut Vec<u8>, entries: &[(i64, i64, i32)], extra_bytes: usize) {
    frame.extend(((20 + extra_bytes) as u16).to_le_bytes());
    frame.extend((entries.len() as u16).to_le_bytes());
    for &(px, sz, count) in entries {
        push_i64(frame, px);
        push_i64(frame, sz);
        frame.extend(count.to_le_bytes());
        frame.extend(vec![0xaa; extra_bytes]);
    }
}

fn book(template: u16, root_extra: usize, group_extra: usize) -> Vec<u8> {
    let mut frame = header(template, (42 + root_extra) as u16);
    for value in [211_874, 1_000, 1_001, 9_007_199_254_740_993, -1] {
        push_i64(&mut frame, value);
    }
    frame.extend([(-2i8) as u8, (-3i8) as u8]);
    frame.extend(vec![0xbb; root_extra]);
    append_group(
        &mut frame,
        &[(12_345, 1_250, 3), (12_346, 12, 1)],
        group_extra,
    );
    append_group(&mut frame, &[(12_300, 123_456, 2)], group_extra);
    frame
}

fn snapshot() -> Vec<u8> {
    let mut frame = header(1006, 26);
    for value in [211_874, 1_000, 99] {
        push_i64(&mut frame, value);
    }
    frame.extend([(-2i8) as u8, (-3i8) as u8]);
    append_group(&mut frame, &[(20_000, 12_345, 1)], 0);
    append_group(&mut frame, &[], 0);
    frame
}

fn exponent(template: u16) -> Vec<u8> {
    let mut frame = header(template, 42);
    for value in [211_874, 1_000, 1_001, 5, 4] {
        push_i64(&mut frame, value);
    }
    frame.extend([(-2i8) as u8, (-3i8) as u8]);
    frame
}

fn trade() -> Vec<u8> {
    let mut frame = header(1005, 62);
    for value in [
        211_874,
        1_000,
        1_001,
        17,
        -12_345,
        123,
        9_007_199_254_740_993,
    ] {
        push_i64(&mut frame, value);
    }
    frame.extend(2i16.to_le_bytes());
    frame.extend([1, (-2i8) as u8, 2, 1]); // BUY, px exponent, sz exponent, ELP
    frame
}

#[test]
fn decodes_bbo_exactly_without_rounding_i64_or_decimal_values() {
    let got = decode_sbe(&bbo()).expect("valid BBO event");
    assert_eq!(got["templateId"], 1000);
    assert_eq!(got["event"], "BboTbtChannelEvent");
    assert_eq!(got["instIdCode"], "211874");
    assert_eq!(got["seqId"], "9007199254740993");
    assert_eq!(got["askPxMantissa"], "123456");
    assert_eq!(got["askPx"], "1234.56");
    assert_eq!(got["askSz"], "1.005");
    assert_eq!(got["bidPx"], "1234.00");
    assert_eq!(got["bidSz"], "0.025");
    assert_eq!(got["askOrdCount"], 2);
    assert_eq!(got["pxExponent"], -2);
}

#[test]
fn formats_extreme_signed_mantissa_with_more_fractional_digits_than_digits() {
    let mut frame = bbo();
    frame[8 + 32..8 + 40].copy_from_slice(&i64::MIN.to_le_bytes());
    frame[8 + 72] = (-20i8) as u8;
    let got = decode_sbe(&frame).expect("valid minimum mantissa");
    assert_eq!(got["askPxMantissa"], "-9223372036854775808");
    assert_eq!(got["askPx"], "-0.09223372036854775808");
}

#[test]
fn decodes_both_repeating_book_sides_and_skips_extended_fixed_blocks() {
    for (id, name) in [
        (1001, "BooksL2TbtChannelEvent"),
        (1003, "BooksL2TbtElpChannelEvent"),
    ] {
        let got = decode_sbe(&book(id, 3, 4)).expect("valid extended order book");
        assert_eq!(got["event"], name);
        assert_eq!(got["prevSeqId"], "-1");
        assert_eq!(
            got["asks"],
            json!([
                {"pxMantissa":"12345","szMantissa":"1250","ordCount":3,"px":"123.45","sz":"1.250"},
                {"pxMantissa":"12346","szMantissa":"12","ordCount":1,"px":"123.46","sz":"0.012"}
            ])
        );
        assert_eq!(
            got["bids"],
            json!([
                {"pxMantissa":"12300","szMantissa":"123456","ordCount":2,"px":"123.00","sz":"123.456"}
            ])
        );
    }
}

#[test]
fn decodes_snapshot_and_empty_order_book_group() {
    let got = decode_sbe(&snapshot()).expect("valid snapshot");
    assert_eq!(got["event"], "SnapshotDepthResponseEvent");
    assert_eq!(got["seqId"], "99");
    assert_eq!(got["asks"][0]["px"], "200.00");
    assert_eq!(got["asks"][0]["sz"], "12.345");
    assert_eq!(got["bids"], json!([]));
}

#[test]
fn decodes_both_exponent_updates_without_fabricating_levels() {
    for (id, name) in [
        (1002, "BooksL2TbtExponentUpdateEvent"),
        (1004, "BooksL2TbtElpExponentUpdateEvent"),
    ] {
        let got = decode_sbe(&exponent(id)).expect("valid exponent update");
        assert_eq!(got["event"], name);
        assert_eq!(got["seqId"], "5");
        assert_eq!(got["prevSeqId"], "4");
        assert_eq!(got["pxExponent"], -2);
        assert!(got.get("asks").is_none());
    }
}

#[test]
fn decodes_trade_enums_and_signed_or_positive_exponents() {
    let got = decode_sbe(&trade()).expect("valid trade");
    assert_eq!(got["event"], "TradesChannelEvent");
    assert_eq!(got["tradeId"], "9007199254740993");
    assert_eq!(got["pxMantissa"], "-12345");
    assert_eq!(got["px"], "-123.45");
    assert_eq!(got["sz"], "12300");
    assert_eq!(got["count"], 2);
    assert_eq!(got["side"], "BUY");
    assert_eq!(got["source"], "ELP");
}

#[test]
fn rejects_each_truncation_of_a_valid_order_book_frame() {
    let full = book(1001, 3, 4);
    for end in 0..full.len() {
        assert!(
            decode_sbe(&full[..end]).is_err(),
            "accepted truncated frame at {end}"
        );
    }
    assert!(decode_sbe(&full).is_ok());
}

#[test]
fn rejects_unknown_schema_template_version_and_enums() {
    let mut invalid = bbo();
    invalid[4..6].copy_from_slice(&2u16.to_le_bytes());
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = bbo();
    invalid[2..4].copy_from_slice(&999u16.to_le_bytes());
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = bbo();
    invalid[6..8].copy_from_slice(&1u16.to_le_bytes());
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = trade();
    invalid[8 + 58] = 2; // side has only SELL=0 and BUY=1
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = trade();
    invalid[8 + 61] = 2; // source has only NORMAL=0 and ELP=1
    assert!(decode_sbe(&invalid).is_err());
}

#[test]
fn rejects_short_fixed_blocks_overstated_groups_and_trailing_data() {
    let mut invalid = bbo();
    invalid[..2].copy_from_slice(&73u16.to_le_bytes());
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = book(1001, 0, 0);
    invalid[8 + 42..8 + 44].copy_from_slice(&19u16.to_le_bytes());
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = book(1001, 0, 0);
    invalid[8 + 44..8 + 46].copy_from_slice(&u16::MAX.to_le_bytes());
    assert!(decode_sbe(&invalid).is_err());
    let mut invalid = snapshot();
    append_group(&mut invalid, &[], 0); // unexpected third group
    assert!(decode_sbe(&invalid).is_err());
}
