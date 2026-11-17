use safe_cex_engine::{ErrorCause, ErrorKind, VenueError};

#[test]
fn ordinary_history_error_wire_has_no_row_detail() {
    let error = VenueError::new("history rejected")
        .kind(ErrorKind::Internal)
        .cause_code(ErrorCause::HistoryDecode);
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        r#"{"message":"history rejected","kind":"internal","causeCode":"HISTORY_DECODE"}"#
    );
}

#[cfg(feature = "history-row-diagnostics")]
#[test]
fn enabled_row_detail_is_closed_and_keeps_the_existing_error_wire() {
    use safe_cex_engine::{HistoryDecodeReason, HistorySchema};
    let error = VenueError::new("history rejected").history_decode(
        HistorySchema::KrakenOrders,
        HistoryDecodeReason::FilledQuantity,
    );
    assert_eq!(
        serde_json::to_string(&error).unwrap(),
        r#"{"message":"history rejected","kind":"internal","causeCode":"HISTORY_DECODE","historyDecode":{"schema":"krakenOrders","reason":"filledQuantity"}}"#
    );
}

#[cfg(feature = "history-row-diagnostics")]
#[test]
fn diagnostic_shape_does_not_expand_inline_error_storage() {
    use safe_cex_engine::HistoryDecode;
    assert!(std::mem::size_of::<HistoryDecode>() <= 2);
    assert!(std::mem::size_of::<Option<HistoryDecode>>() <= 2);
}

#[cfg(feature = "history-row-diagnostics")]
#[test]
fn every_simple_schema_and_reason_keeps_its_existing_wire() {
    use safe_cex_engine::{HistoryDecode, HistoryDecodeReason as R, HistorySchema as S};
    for schema in [
        S::BitgetClassicFills,
        S::BitgetUtaFills,
        S::BitgetClassicOrders,
        S::BitgetUtaOrders,
        S::BitgetClassicIncome,
        S::BitgetUtaIncome,
        S::BitgetPublicTrades,
        S::KrakenFills,
        S::KrakenOrders,
        S::KrakenIncome,
        S::BingxIncome,
        S::BingxOrders,
    ] {
        for reason in [
            R::RowShape,
            R::Identity,
            R::Symbol,
            R::Quantity,
            R::FilledQuantity,
            R::Timestamp,
            R::TimestampOrder,
            R::Side,
            R::OrderStatus,
            R::OrderType,
            R::Fee,
            R::Market,
            R::Economics,
            R::QuantityNegative,
            R::QuantityZero,
            R::FilledQuantityNegative,
            R::FilledQuantityExceedsQuantity,
            R::QuantityDecimal,
            R::FilledQuantityDecimal,
            R::TimestampDecimal,
            R::LastUpdateTimestampDecimal,
            R::LimitPriceDecimal,
            R::FieldType,
            R::FieldValue,
            R::EnvelopeShape,
            R::EnvelopeCode,
            R::DataShape,
        ] {
            assert_eq!(
                serde_json::to_value(HistoryDecode::new(schema, reason)).unwrap(),
                serde_json::json!({"schema":schema, "reason":reason})
            );
        }
    }
}

#[cfg(feature = "history-row-diagnostics")]
#[test]
fn compact_bingx_detail_preserves_each_closed_wire_component() {
    use safe_cex_engine::{
        HistoryDecode, HistoryDecodeField as F, HistoryDecodeReason as R, HistoryDecodeShape,
        HistoryEnvelopeCode as C, HistoryExpectedType as E, HistoryJsonType as A,
        HistorySchema as S,
    };
    for schema in [S::BingxIncome, S::BingxOrders] {
        for (field, expected) in [
            (F::Root, E::Object),
            (F::Code, E::Integer),
            (F::Message, E::String),
            (
                F::Data,
                if schema == S::BingxOrders {
                    E::ArrayOrObject
                } else {
                    E::Array
                },
            ),
            (F::DataOrders, E::Array),
            (F::Row, E::Object),
            (F::Symbol, E::String),
            (F::IncomeType, E::String),
            (F::Income, E::NumberOrNumericString),
            (F::Asset, E::String),
            (F::Time, E::NumberOrNumericString),
            (F::TranId, E::StringOrInteger),
            (F::TradeId, E::StringOrIntegerOrNull),
            (F::OrderId, E::StringOrInteger),
            (F::Side, E::String),
            (F::OrderType, E::String),
            (F::Status, E::String),
            (F::Price, E::NumberOrNumericString),
            (F::AveragePrice, E::NumberOrNumericString),
            (F::Quantity, E::NumberOrNumericString),
            (F::FilledQuantity, E::NumberOrNumericString),
            (F::ReduceOnly, E::BooleanOrStringOrNull),
            (F::PositionSide, E::StringOrNull),
            (F::UpdateTime, E::NumberOrNumericString),
        ] {
            for actual in [
                A::Missing,
                A::InvalidJson,
                A::Null,
                A::Boolean,
                A::Number,
                A::String,
                A::Array,
                A::Object,
            ] {
                for envelope_code in [C::Present, C::Absent, C::Unavailable] {
                    for reason in [
                        R::RowShape,
                        R::FieldType,
                        R::FieldValue,
                        R::EnvelopeShape,
                        R::EnvelopeCode,
                        R::DataShape,
                        R::Side,
                        R::OrderType,
                        R::OrderStatus,
                    ] {
                        let shape = HistoryDecodeShape {
                            field,
                            expected,
                            actual,
                            envelope_code,
                        };
                        let detail = HistoryDecode::with_shape(schema, reason, shape).unwrap();
                        assert_eq!(
                            serde_json::to_value(detail).unwrap(),
                            serde_json::json!({"schema":schema, "reason":reason, "shape":shape})
                        );
                    }
                }
            }
        }
    }
    let shape = HistoryDecodeShape {
        field: F::Income,
        expected: E::String,
        actual: A::Null,
        envelope_code: C::Present,
    };
    assert!(HistoryDecode::with_shape(S::BingxIncome, R::FieldType, shape).is_none());
    assert!(HistoryDecode::with_shape(S::KrakenIncome, R::FieldType, shape).is_none());
}
