use safe_cex_types::HistoricalTradeOptions;

// ts: USER_TRADES_MAX_LIMIT
pub const USER_TRADES_MAX_LIMIT: f64 = 1000.0;
// ts: USER_TRADES_MAX_WINDOW_MS
pub const USER_TRADES_MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 60.0 * 60.0 * 1000.0;

// ts: buildUserTradesParams return object; numbers stay f64 until the wire boundary.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTradesParams<'a> {
    pub symbol: &'a str,
    pub limit: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_id: Option<f64>,
}

// ts: buildUserTradesParams
pub fn build_user_trades_params(opts: &HistoricalTradeOptions) -> UserTradesParams<'_> {
    UserTradesParams {
        symbol: &opts.symbol,
        limit: opts
            .limit
            .unwrap_or(USER_TRADES_MAX_LIMIT)
            .clamp(1.0, USER_TRADES_MAX_LIMIT),
        order_id: opts.order_id.as_deref().filter(|id| !id.is_empty()),
        start_time: opts.from,
        end_time: opts.to,
        from_id: if opts.from.is_none() && opts.to.is_none() {
            opts.from_id
        } else {
            None
        },
    }
}
