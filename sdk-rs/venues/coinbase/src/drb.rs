//! Shared JSON-RPC transport for the Coinbase derivatives gateway.
//! Consumers own their pending contexts and forward replies/loss to this transport.
use safe_cex_engine::{
    Cx, ErrorKind, Method, RequestId, Response, SocketId,
    classify::{self, CodeTable, Intent, Key, Native, NativeCode, Released, Row, Verdict},
    request_context::{Context, Operation, Session},
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};

pub const HTTP_URL: &str = "https://drb.coinbase.com/api/v2";
pub const WS_URL: &str = "wss://drb.coinbase.com/ws/api/v2";
pub const PUBLIC_WS_URL: &str = "wss://streams.drb.coinbase.com/ws/api/v2";
pub const MAX_FRAME_BYTES: usize = 32 * 1024;
pub const REQUEST_TIMEOUT_MS: u32 = 30_000;
const MAX_PENDING: usize = 256;
const MAX_RPC_ID: u64 = (1 << 53) - 1;
pub type RpcId = u64;

/// HTTP bearer sessions and authenticated main WebSocket sessions are distinct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    Http,
    WebSocket(SocketId),
}

/// A structured gateway error. Debug deliberately omits untrusted text/data.
#[derive(Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}
impl std::fmt::Debug for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcError")
            .field("code", &self.code)
            .finish_non_exhaustive()
    }
}

/// Released requests never grant automatic resend permission.
#[derive(Debug)]
pub struct Failure {
    pub verdict: Verdict,
    pub code: Option<i64>,
    pub rpc_error: Option<RpcError>,
}
impl Failure {
    fn unknown(kind: Option<ErrorKind>) -> Self {
        Self {
            verdict: Verdict::Unknown(kind),
            code: None,
            rpc_error: None,
        }
    }
}

// The COMPLETE code table is authoritative; the introductory summary on the
// same page assigns different meanings to several numbers. Unknown codes stay
// uncertain. https://docs.deribit.com/articles/errors#complete-rpc-error-codes-reference
const fn refusal(code: i64, kind: ErrorKind) -> Row {
    Row {
        key: Key::Int(code),
        verdict: Verdict::Refused(kind),
        only: None,
    }
}
pub(crate) const CODES: CodeTable = CodeTable {
    rows: &[
        refusal(-32700, ErrorKind::InvalidInput),
        refusal(-32600, ErrorKind::InvalidInput),
        refusal(-32601, ErrorKind::Unsupported),
        refusal(-32602, ErrorKind::InvalidInput),
        refusal(10000, ErrorKind::AuthInvalid),
        refusal(10002, ErrorKind::InvalidInput),
        refusal(10003, ErrorKind::Rejected),
        refusal(10004, ErrorKind::Rejected),
        refusal(10005, ErrorKind::InvalidInput),
        refusal(10006, ErrorKind::InvalidInput),
        refusal(10007, ErrorKind::InvalidInput),
        refusal(10009, ErrorKind::InsufficientBalance),
        refusal(10010, ErrorKind::Rejected),
        refusal(10011, ErrorKind::InvalidInput),
        refusal(10012, ErrorKind::Rejected),
        refusal(10019, ErrorKind::AuthPermission),
        refusal(10020, ErrorKind::InvalidInput),
        refusal(10021, ErrorKind::InvalidInput),
        refusal(10022, ErrorKind::InvalidInput),
        refusal(10023, ErrorKind::InvalidInput),
        refusal(10024, ErrorKind::InvalidInput),
        refusal(10025, ErrorKind::InvalidInput),
        refusal(10026, ErrorKind::InvalidInput),
        refusal(10027, ErrorKind::InvalidInput),
        refusal(10028, ErrorKind::RateLimited),
        refusal(10029, ErrorKind::AuthPermission),
        refusal(10030, ErrorKind::InvalidInput),
        refusal(10031, ErrorKind::InvalidInput),
        refusal(10033, ErrorKind::Unsupported),
        refusal(10039, ErrorKind::InsufficientBalance),
        refusal(10043, ErrorKind::InvalidInput),
        refusal(10044, ErrorKind::InvalidInput),
        refusal(10047, ErrorKind::RateLimited),
        refusal(10066, ErrorKind::RateLimited),
        refusal(11008, ErrorKind::Rejected),
        refusal(11029, ErrorKind::InvalidInput),
        refusal(11042, ErrorKind::AuthPermission),
        refusal(11043, ErrorKind::InvalidInput),
        refusal(11044, ErrorKind::Rejected),
        refusal(13009, ErrorKind::AuthExpired),
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};

#[derive(Default)]
enum Field<T> {
    #[default]
    Missing,
    Present(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Present)
    }
}
#[derive(Deserialize)]
struct Envelope {
    jsonrpc: String,
    #[serde(default)]
    id: Field<RpcId>,
    // Preserve result:null, reject duplicate envelope fields via struct serde.
    #[serde(default)]
    result: Field<Value>,
    #[serde(default)]
    error: Field<RpcError>,
}

struct InFlight {
    channel: Channel,
    deadline: f64,
    intent: Intent,
    pool: usize,
    identifierless_http_read: bool,
}

struct Bucket {
    credits: f64,
    cap: f64,
    cost: f64,
    last: Option<f64>,
}
impl Bucket {
    fn new(rate: f64, burst: f64) -> Self {
        let cost = 10_000. / rate;
        let cap = cost * burst;
        Self {
            credits: cap,
            cap,
            cost,
            last: None,
        }
    }
    fn refill(&mut self, now: f64) {
        // A regressed clock neither earns credits nor moves the watermark.
        if let Some(last) = self.last {
            self.credits = (self.credits + (now - last).max(0.) * 10.).min(self.cap);
            self.last = Some(now.max(last));
        } else {
            self.last = Some(now);
        }
    }
}

/// Credit admission and bounded in-flight correlation. No internal resend queue.
/// Pools follow https://docs.deribit.com/articles/rate-limits; matching defaults
/// to the lowest tier until the account stream supplies its documented limits.
pub struct Transport {
    next_id: RpcId,
    in_flight: BTreeMap<RpcId, InFlight>,
    // general, matching, instruments, subscriptions, transaction log, positions
    pools: [Bucket; 6],
    position_moves: VecDeque<f64>,
}
impl Default for Transport {
    fn default() -> Self {
        Self {
            next_id: 1,
            in_flight: BTreeMap::new(),
            pools: [
                Bucket::new(20., 100.),
                Bucket::new(5., 20.),
                Bucket::new(1., 50.),
                Bucket {
                    credits: 30_000.,
                    cap: 30_000.,
                    cost: 3000.,
                    last: None,
                },
                Bucket::new(1., 8.),
                Bucket::new(0.1, 6.),
            ],
            position_moves: VecDeque::new(),
        }
    }
}

impl Transport {
    /// Apply documented account-summary limits without replenishing used credits.
    pub fn set_matching_limits(&mut self, rate: f64, burst: f64) -> Result<(), String> {
        if !rate.is_finite()
            || !burst.is_finite()
            || rate <= 0.
            || burst < 1.
            || !(10_000. / rate * burst).is_finite()
        {
            return Err("drb invalid matching limits".into());
        }
        let pool = &mut self.pools[1];
        let available_requests = pool.credits / pool.cost;
        pool.cost = 10_000. / rate;
        pool.cap = pool.cost * burst;
        pool.credits = (available_requests * pool.cost).min(pool.cap);
        Ok(())
    }

    /// Queue under the caller's engine-issued authority. The caller retains the
    /// context in its pending state and validates/finishes it on delivery.
    /// Private bearer headers are supplied by Venue::session_bearer at prepare.
    pub fn http<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        context: &Context,
        method: &str,
        params: Value,
        session: Option<&Session>,
        pending: P,
    ) -> Result<(RpcId, RequestId), String> {
        let intent = method_intent(method)?;
        if method.starts_with("private/") && session.is_none() {
            return Err("drb HTTP private request requires an engine bearer session".into());
        }
        if intent != Intent::Read && context.operation() != Operation::Mutation {
            return Err("drb HTTP mutation requires a Mutation context".into());
        }
        // The production chart route is GET-only and its response has no RPC ID.
        // Keep credit admission, timeout and engine HTTP correlation in this transport.
        let chart_get = method == "public/get_tradingview_chart_data";
        let mut url = format!("{HTTP_URL}/{method}");
        if chart_get {
            let params = params.as_object().ok_or("drb invalid chart parameters")?;
            for (index, (key, value)) in params.iter().enumerate() {
                url.push(if index == 0 { '?' } else { '&' });
                safe_cex_engine::url::encode_into(&mut url, key);
                url.push('=');
                let value = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    _ => return Err("drb invalid chart parameter value".into()),
                };
                safe_cex_engine::url::encode_into(&mut url, &value);
            }
        }
        let (id, body) = self.prepare(Channel::Http, method, intent, params, cx.now)?;
        let request = match cx.request_managed_with_headers(
            context.clone(),
            session,
            if chart_get { Method::Get } else { Method::Post },
            url,
            &[("Content-Type", "application/json")],
            (!chart_get).then_some(body),
            pending,
        ) {
            Ok(request) => request,
            Err((_, error)) => {
                // Admission failed before physical release. Retire correlation;
                // credit estimates remain conservative and no resend is queued.
                self.in_flight.remove(&id);
                return Err(error.to_string());
            }
        };
        cx.request_timeout(request, REQUEST_TIMEOUT_MS);
        Ok((id, request))
    }

    /// Provider-backed login retains a signed recipe inside the managed request;
    /// attaching an unmanaged signing action after queuing would be too late.
    pub(crate) fn http_auth<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        context: &Context,
        params: Value,
        pending: P,
        step: impl FnOnce(RpcId) -> Box<dyn safe_cex_engine::SignStep>,
    ) -> Result<(RpcId, RequestId), String> {
        let (id, body) =
            self.prepare(Channel::Http, "public/auth", Intent::Read, params, cx.now)?;
        let request = match cx.request_signed_managed(
            context.clone(),
            None,
            Method::Post,
            format!("{HTTP_URL}/public/auth"),
            Some(body),
            step(id),
            pending,
        ) {
            Ok(request) => request,
            Err((_, error)) => {
                self.in_flight.remove(&id);
                return Err(error.to_string());
            }
        };
        cx.request_timeout(request, REQUEST_TIMEOUT_MS);
        Ok((id, request))
    }

    /// Caller must gate private methods on its Session and use the main WS host.
    pub fn websocket<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        socket: SocketId,
        method: &str,
        params: Value,
    ) -> Result<RpcId, String> {
        if socket == SocketId(0) {
            return Err("drb invalid socket".into());
        }
        let intent = method_intent(method)?;
        let (id, body) =
            self.prepare(Channel::WebSocket(socket), method, intent, params, cx.now)?;
        cx.send(socket, body);
        Ok(id)
    }

    /// Provider-backed authentication is read-only and pinned to this socket.
    /// Mutation methods cannot enter the generic signed-auth queue.
    pub(crate) fn websocket_auth<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        socket: SocketId,
        params: Value,
        step: impl FnOnce(RpcId) -> Box<dyn safe_cex_engine::SignStep>,
    ) -> Result<RpcId, String> {
        if socket == SocketId(0) {
            return Err("drb invalid socket".into());
        }
        let (id, body) = self.prepare(
            Channel::WebSocket(socket),
            "public/auth",
            Intent::Read,
            params,
            cx.now,
        )?;
        cx.send_signed(socket, body, step(id));
        Ok(id)
    }

    fn prepare(
        &mut self,
        channel: Channel,
        method: &str,
        intent: Intent,
        params: Value,
        now: f64,
    ) -> Result<(RpcId, String), String> {
        if !params.is_object() || !now.is_finite() || now < 0. {
            return Err("drb invalid request".into());
        }
        if self.next_id > MAX_RPC_ID || self.in_flight.len() >= MAX_PENDING {
            return Err("drb request capacity exhausted".into());
        }
        let id = self.next_id;
        let body = serde_json::to_string(
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .map_err(|_| "drb request encoding failed")?;
        if body.len() > MAX_FRAME_BYTES {
            return Err("drb frame exceeds 32 KiB".into());
        }
        let pool = pool_for(method);
        self.pools[pool].refill(now);
        if self.pools[pool].credits < self.pools[pool].cost {
            return Err("drb credit budget exhausted; defer unsent request".into());
        }
        if pool == 5 {
            const WEEK_MS: f64 = 168. * 60. * 60. * 1000.;
            while self
                .position_moves
                .front()
                .is_some_and(|at| now - at >= WEEK_MS)
            {
                self.position_moves.pop_front();
            }
            if self.position_moves.len() >= 100 {
                return Err("drb weekly position move limit exhausted".into());
            }
            self.position_moves.push_back(now);
        }
        self.pools[pool].credits -= self.pools[pool].cost;
        self.next_id += 1;
        self.in_flight.insert(
            id,
            InFlight {
                channel,
                deadline: now + f64::from(REQUEST_TIMEOUT_MS),
                intent,
                pool,
                identifierless_http_read: channel == Channel::Http
                    && method == "public/get_tradingview_chart_data",
            },
        );
        Ok((id, body))
    }

    /// HTTP supplies its status; WS supplies 200. A mismatched/malformed response
    /// retires only the expected request with Unknown; it never proves refusal.
    /// Timestamped completion for engine callbacks; receipt time anchors credit
    /// cooldown and rejects acknowledgements after the request deadline.
    pub fn response_at(
        &mut self,
        id: RpcId,
        status: u16,
        body: &str,
        now_ms: f64,
    ) -> Result<Value, Failure> {
        self.response_inner(id, status, body, Some(now_ms))
    }

    /// Compatibility completion when no receipt timestamp is available. On rate
    /// exhaustion, conservatively anchor cooldown at the request deadline.
    pub fn response(&mut self, id: RpcId, status: u16, body: &str) -> Result<Value, Failure> {
        self.response_inner(id, status, body, None)
    }

    fn response_inner(
        &mut self,
        id: RpcId,
        status: u16,
        body: &str,
        now_ms: Option<f64>,
    ) -> Result<Value, Failure> {
        let Some(pending) = self.in_flight.remove(&id) else {
            return Err(Failure::unknown(None));
        };
        if let Some(now) = now_ms {
            if !now.is_finite() || now < pending.deadline - f64::from(REQUEST_TIMEOUT_MS) {
                return Err(Failure::unknown(Some(ErrorKind::Internal)));
            }
            if now >= pending.deadline {
                return Err(Failure::unknown(Some(ErrorKind::Timeout)));
            }
        }
        if matches!(pending.channel, Channel::WebSocket(_)) && body.len() > MAX_FRAME_BYTES {
            return Err(Failure::unknown(Some(ErrorKind::Internal)));
        }
        let response = Response {
            status,
            headers: "",
            body,
        };
        let envelope = serde_json::from_str::<Envelope>(body).ok().filter(|e| {
            e.jsonrpc == "2.0"
                && match &e.id {
                    Field::Present(actual) => *actual == id,
                    Field::Missing => pending.identifierless_http_read,
                }
        });
        let Some(envelope) = envelope else {
            return Err(Failure {
                verdict: classify::verdict(
                    &CODES,
                    pending.intent,
                    Released::Response(&response),
                    None,
                ),
                code: None,
                rpc_error: None,
            });
        };
        match (envelope.result, envelope.error) {
            (Field::Present(result), Field::Missing) if (200..300).contains(&status) => Ok(result),
            (Field::Missing, Field::Present(error)) => {
                let verdict = classify::verdict(
                    &CODES,
                    pending.intent,
                    Released::Response(&response),
                    Some(&Native {
                        code: NativeCode::Int(error.code),
                        message: &error.message,
                    }),
                );
                if matches!(error.code, 10028 | 10066) {
                    // Other clients/IP traffic can consume the shared server budget.
                    // Deplete our estimate and require fresh credit admission.
                    let pool = &mut self.pools[pending.pool];
                    pool.refill(now_ms.unwrap_or(pending.deadline));
                    pool.credits = 0.;
                }
                Err(Failure {
                    verdict,
                    code: Some(error.code),
                    rpc_error: Some(error),
                })
            }
            _ => Err(Failure {
                verdict: classify::verdict(
                    &CODES,
                    pending.intent,
                    Released::Response(&response),
                    None,
                ),
                code: None,
                rpc_error: None,
            }),
        }
    }

    pub fn failed(&mut self, id: RpcId, kind: ErrorKind) -> Option<Failure> {
        self.in_flight
            .remove(&id)
            .map(|_| Failure::unknown(Some(kind)))
    }
    pub fn disconnected(&mut self, socket: SocketId) -> Vec<(RpcId, Failure)> {
        let ids: Vec<_> = self
            .in_flight
            .iter()
            .filter_map(|(&id, p)| (p.channel == Channel::WebSocket(socket)).then_some(id))
            .collect();
        ids.into_iter()
            .filter_map(|id| self.failed(id, ErrorKind::Network).map(|f| (id, f)))
            .collect()
    }
    pub fn expire(&mut self, now_ms: f64) -> Vec<(RpcId, Failure)> {
        let ids: Vec<_> = self
            .in_flight
            .iter()
            .filter_map(|(&id, p)| (now_ms.is_finite() && now_ms >= p.deadline).then_some(id))
            .collect();
        ids.into_iter()
            .filter_map(|id| self.failed(id, ErrorKind::Timeout).map(|f| (id, f)))
            .collect()
    }
}

// Closed semantic contract, independent of rate pools. New methods must be
// classified here and pinned in the foundation authority table before use.
// Authentication/heartbeat/subscription control is Read; account and trading writes are not.
pub(crate) fn method_intent(method: &str) -> Result<Intent, String> {
    Ok(match method {
        "private/buy" | "private/sell" | "private/close_position" => Intent::Place,
        "private/cancel"
        | "private/cancel_all"
        | "private/cancel_all_by_currency"
        | "private/cancel_all_by_currency_pair"
        | "private/cancel_all_by_instrument"
        | "private/cancel_all_by_kind_or_type"
        | "private/cancel_by_label" => Intent::Cancel,
        "private/edit" | "private/edit_by_label" => Intent::Amend,
        "private/change_margin_model"
        | "private/create_combo"
        | "private/disable_cancel_on_disconnect"
        | "private/enable_cancel_on_disconnect"
        | "private/logout"
        | "private/move_positions"
        | "private/set_leverage"
        | "private/submit_transfer_between_subaccounts" => Intent::Setting,
        "private/get_access_log"
        | "private/get_account_summaries"
        | "private/get_account_summary"
        | "private/get_cancel_on_disconnect"
        | "private/get_leg_prices"
        | "private/get_leverage"
        | "private/get_margins"
        | "private/get_max_order_size"
        | "private/get_open_orders"
        | "private/get_open_orders_by_currency"
        | "private/get_open_orders_by_instrument"
        | "private/get_open_orders_by_label"
        | "private/get_order_history_by_currency"
        | "private/get_order_history_by_instrument"
        | "private/get_order_margin_by_ids"
        | "private/get_order_state"
        | "private/get_order_state_by_label"
        | "private/get_pme_params"
        | "private/get_position"
        | "private/get_positions"
        | "private/get_risk_profile"
        | "private/get_settlement_history_by_currency"
        | "private/get_settlement_history_by_instrument"
        | "private/get_subaccounts"
        | "private/get_subaccounts_details"
        | "private/get_trading_limits"
        | "private/get_transaction_log"
        | "private/get_trigger_order_history"
        | "private/get_user_locks"
        | "private/get_user_trades_by_currency"
        | "private/get_user_trades_by_currency_and_time"
        | "private/get_user_trades_by_instrument"
        | "private/get_user_trades_by_instrument_and_time"
        | "private/get_user_trades_by_order"
        | "private/simulate_portfolio"
        | "private/subscribe"
        | "private/unsubscribe"
        | "private/unsubscribe_all"
        | "public/auth"
        | "public/disable_heartbeat"
        | "public/get_announcements"
        | "public/get_book_summary_by_currency"
        | "public/get_book_summary_by_instrument"
        | "public/get_combo_details"
        | "public/get_combo_ids"
        | "public/get_combos"
        | "public/get_contract_size"
        | "public/get_currencies"
        | "public/get_delivery_prices"
        | "public/get_expirations"
        | "public/get_funding_chart_data"
        | "public/get_funding_rate_history"
        | "public/get_funding_rate_value"
        | "public/get_historical_volatility"
        | "public/get_index_chart_data"
        | "public/get_index_price"
        | "public/get_index_price_names"
        | "public/get_instrument"
        | "public/get_instruments"
        | "public/get_last_settlements_by_currency"
        | "public/get_last_settlements_by_instrument"
        | "public/get_last_trades_by_currency"
        | "public/get_last_trades_by_currency_and_time"
        | "public/get_last_trades_by_instrument"
        | "public/get_last_trades_by_instrument_and_time"
        | "public/get_mark_price_history"
        | "public/get_order_book"
        | "public/get_order_book_by_instrument_id"
        | "public/get_supported_index_names"
        | "public/get_time"
        | "public/get_trade_volumes"
        | "public/get_tradingview_chart_data"
        | "public/get_volatility_index_data"
        | "public/hello"
        | "public/set_heartbeat"
        | "public/status"
        | "public/subscribe"
        | "public/test"
        | "public/ticker"
        | "public/tickers_by_currency"
        | "public/unsubscribe"
        | "public/unsubscribe_all" => Intent::Read,
        // Mass quotes have additional quotas unsupported by S0. All other
        // unlisted methods also fail closed, including plausible get_/set_ names.
        _ => return Err("drb unsupported method".into()),
    })
}

fn pool_for(method: &str) -> usize {
    match method {
        "public/get_instruments" => 2,
        "public/subscribe" | "private/subscribe" => 3,
        "private/get_transaction_log" => 4,
        "private/move_positions" => 5,
        "private/buy"
        | "private/sell"
        | "private/edit"
        | "private/edit_by_label"
        | "private/close_position" => 1,
        name if name.starts_with("private/cancel") => 1,
        _ => 0,
    }
}

fn map_symbol(value: &str, suffix: &str, target: &str) -> Result<String, String> {
    let base = value.strip_suffix(suffix).filter(|base| {
        !base.is_empty()
            && base.len() <= 64
            && base
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    });
    base.map(|base| format!("{base}{target}"))
        .ok_or_else(|| "drb invalid perpetual instrument alias".into())
}
pub fn to_instrument(symbol: &str) -> Result<String, String> {
    map_symbol(symbol, "-PERP-INTX", "_USDC-PERPETUAL")
}
pub fn from_instrument(instrument: &str) -> Result<String, String> {
    map_symbol(instrument, "_USDC-PERPETUAL", "-PERP-INTX")
}
