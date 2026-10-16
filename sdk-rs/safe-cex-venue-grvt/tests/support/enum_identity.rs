pub(super) trait NativeEnum {
    fn native_wire(&self) -> &'static str;
}
macro_rules! enum_identity {
    ($ty:ty { $($variant:ident => $wire:literal),* $(,)? }) => {
        impl NativeEnum for $ty {
            fn native_wire(&self) -> &'static str {
                match self { $(Self::$variant => $wire),* }
            }
        }
    };
}
enum_identity!(safe_cex_venue_grvt::types::GrvtOrderSide {
    Buy => "BUY",
    Sell => "SELL",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtOrderType {
    Limit => "LIMIT",
    Market => "MARKET",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtTimeInForce {
    GoodTillTime => "GOOD_TILL_TIME",
    AllOrNone => "ALL_OR_NONE",
    ImmediateOrCancel => "IMMEDIATE_OR_CANCEL",
    FillOrKill => "FILL_OR_KILL",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtCandlestickInterval {
    Ci1M => "CI_1_M",
    Ci3M => "CI_3_M",
    Ci5M => "CI_5_M",
    Ci15M => "CI_15_M",
    Ci30M => "CI_30_M",
    Ci1H => "CI_1_H",
    Ci2H => "CI_2_H",
    Ci4H => "CI_4_H",
    Ci6H => "CI_6_H",
    Ci8H => "CI_8_H",
    Ci12H => "CI_12_H",
    Ci1D => "CI_1_D",
    Ci3D => "CI_3_D",
    Ci5D => "CI_5_D",
    Ci1W => "CI_1_W",
    Ci2W => "CI_2_W",
    Ci3W => "CI_3_W",
    Ci4W => "CI_4_W",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtOrderStatus {
    Pending => "PENDING",
    Open => "OPEN",
    Filled => "FILLED",
    Rejected => "REJECTED",
    Cancelled => "CANCELLED",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtAssetKind {
    Perpetual => "PERPETUAL",
    Future => "FUTURE",
    Call => "CALL",
    Put => "PUT",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtLiquidity {
    Maker => "MAKER",
    Taker => "TAKER",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtJsonRpcVersion {
    V2 => "2.0",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtSubscriptionMethod {
    Subscribe => "subscribe",
    Unsubscribe => "unsubscribe",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtBuilderUnauthorizedReason {
    Unauthorized => "unauthorized",
    CapTooLow => "cap-too-low",
});
enum_identity!(safe_cex_venue_grvt::types::GrvtTriggerKind {
    StopLoss => "STOP_LOSS",
    TakeProfit => "TAKE_PROFIT",
});
enum_identity!(safe_cex_types::OrderStopTriggerType { MarkPrice => "mark_price", LastPrice => "last_price", IndexPrice => "index_price" });
