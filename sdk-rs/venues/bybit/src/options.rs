//! Native option terms, quote queries and account Greeks.
use crate::policy::{entry_policy, read_policy};
use crate::sign::{self, Query};
use crate::trading::codes::BYBIT;
use crate::wire::*;
use crate::{BybitLinear, Category, MARKETS_PATH, Pending, Rest, Then, venue_error};
use safe_cex_engine::classify::{self, Native, NativeCode, ReadRetry, Released};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::{
    DerivativeProduct, Market, OptionChainQuery, OptionGreeks, OptionRight, ProductKind,
};
use safe_cex_engine::request_context::{
    Context, Credential, Operation, Policy, RetryEvidence, RetryPolicy,
};
use safe_cex_engine::{CredentialSlot, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};

type Opt<'a> = Option<Str<'a>>;

pub(crate) const GREEK_UNITS: &str = "Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day";

/// One option query in flight under its command's Read Context.
pub enum Request {
    Chain {
        context: Context,
        query: OptionChainQuery,
        quotes: Vec<Quote>,
        invalid: bool,
    },
    Greeks {
        context: Context,
        rows: Vec<AccountGreeks>,
        invalid: bool,
        resent: bool,
    },
}

impl Request {
    pub(crate) fn context(&self) -> &Context {
        match self {
            Request::Chain { context, .. } | Request::Greeks { context, .. } => context,
        }
    }
}

fn refused(message: &str, kind: ErrorKind) -> VenueError {
    VenueError::new(message)
        .kind(kind)
        .outcome(Outcome::NotSent)
}

fn malformed() -> VenueError {
    VenueError::new("Malformed Bybit option response").kind(ErrorKind::Internal)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    symbol: String,
    bid: f64,
    ask: f64,
    mark: f64,
    mark_iv: f64,
    bid_iv: f64,
    ask_iv: f64,
    underlying_price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    greeks: Option<OptionGreeks>,
}

#[derive(Serialize)]
pub struct AccountGreeks {
    underlying: String,
    #[serde(flatten)]
    greeks: OptionGreeks,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoinGreeks<'a> {
    #[serde(borrow)]
    base_coin: Str<'a>,
    #[serde(borrow, default)]
    total_delta: Opt<'a>,
    #[serde(borrow, default)]
    total_gamma: Opt<'a>,
    #[serde(borrow, default)]
    total_vega: Opt<'a>,
    #[serde(borrow, default)]
    total_theta: Opt<'a>,
}

pub(crate) fn greeks(
    delta: &Opt<'_>,
    gamma: &Opt<'_>,
    vega: &Opt<'_>,
    theta: &Opt<'_>,
) -> Option<OptionGreeks> {
    present(delta).map(|_| OptionGreeks {
        delta: or_zero(delta),
        gamma: or_zero(gamma),
        vega: or_zero(vega),
        theta: or_zero(theta),
        units: GREEK_UNITS.into(),
    })
}

pub(crate) fn on_item(request: &mut Request, item: &str) {
    match request {
        Request::Chain {
            quotes, invalid, ..
        } => {
            if let Ok(row) = serde_json::from_str::<TickerRow>(item) {
                if row.symbol.is_empty()
                    || [
                        &row.bid1_price,
                        &row.ask1_price,
                        &row.mark_price,
                        &row.mark_iv,
                        &row.bid1_iv,
                        &row.ask1_iv,
                        &row.underlying_price,
                        &row.delta,
                        &row.gamma,
                        &row.vega,
                        &row.theta,
                    ]
                    .into_iter()
                    .any(|value| present(value).is_some_and(|value| !float(value).is_finite()))
                {
                    *invalid = true;
                    return;
                }
                quotes.push(Quote {
                    symbol: row.symbol.into_owned(),
                    bid: or_zero(&row.bid1_price),
                    ask: or_zero(&row.ask1_price),
                    mark: or_zero(&row.mark_price),
                    mark_iv: or_zero(&row.mark_iv),
                    bid_iv: or_zero(&row.bid1_iv),
                    ask_iv: or_zero(&row.ask1_iv),
                    underlying_price: or_zero(&row.underlying_price),
                    greeks: greeks(&row.delta, &row.gamma, &row.vega, &row.theta),
                });
            } else {
                *invalid = true;
            }
        }
        Request::Greeks { rows, invalid, .. } => {
            if let Ok(row) = serde_json::from_str::<CoinGreeks>(item) {
                if row.base_coin.is_empty()
                    || [
                        &row.total_delta,
                        &row.total_gamma,
                        &row.total_vega,
                        &row.total_theta,
                    ]
                    .iter()
                    .any(|value| !float_of(value).is_finite())
                {
                    *invalid = true;
                    return;
                }
                rows.push(AccountGreeks {
                    underlying: row.base_coin.into_owned(),
                    greeks: OptionGreeks {
                        delta: or_zero(&row.total_delta),
                        gamma: or_zero(&row.total_gamma),
                        vega: or_zero(&row.total_vega),
                        theta: or_zero(&row.total_theta),
                        units: GREEK_UNITS.into(),
                    },
                });
            } else {
                *invalid = true;
            }
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Terms<'a> {
    underlying: &'a str,
    expiry: f64,
    strike: f64,
    right: &'static str,
    style: &'static str,
    settlement: &'static str,
    premium_asset: &'a str,
    settle_asset: &'a str,
}

#[derive(Serialize)]
struct OptionMarket<'a> {
    #[serde(flatten)]
    market: &'a Market,
    option: Terms<'a>,
}

#[derive(Serialize)]
struct ChainRow<'a> {
    market: OptionMarket<'a>,
    quote: &'a Quote,
}

fn matches(product: &DerivativeProduct, query: &OptionChainQuery) -> bool {
    product.underlying == query.underlying
        && matches!(product.kind, ProductKind::Call | ProductKind::Put)
        && query
            .expiry
            .is_none_or(|expiry| product.expiry == Some(expiry))
        && query.right.is_none_or(|right| {
            matches!(
                (right, product.kind),
                (OptionRight::Call, ProductKind::Call) | (OptionRight::Put, ProductKind::Put)
            )
        })
        && query
            .strike_min
            .is_none_or(|min| product.strike.is_some_and(|strike| strike >= min))
        && query
            .strike_max
            .is_none_or(|max| product.strike.is_some_and(|strike| strike <= max))
}

impl BybitLinear {
    /// A product the account has not enabled is its configuration.
    fn options_enabled(&self, cx: &mut Cx<'_, Pending>, context: &Context) -> bool {
        if self.category_enabled(Category::Option) {
            return true;
        }
        let refusal = refused(
            "Options are not enabled on this Bybit account",
            ErrorKind::InvalidInput,
        );
        let _ = cx.fail_context(context, refusal);
        false
    }

    pub(crate) fn fetch_option_chain(&mut self, cx: &mut Cx<'_, Pending>, query: OptionChainQuery) {
        // The engine owns an initial admission refusal.
        let Ok(context) = cx.admit_http_command(read_policy()) else {
            return;
        };
        if !self.options_enabled(cx, &context) {
            return;
        }
        if query.underlying.is_empty()
            || [query.expiry, query.strike_min, query.strike_max]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite() || value < 0.0)
            || matches!((query.strike_min, query.strike_max), (Some(min), Some(max)) if min > max)
        {
            let refusal = refused("Invalid Bybit option chain query", ErrorKind::InvalidInput);
            let _ = cx.fail_context(&context, refusal);
            return;
        }
        self.select_category(Category::Option);
        let params = Query::new()
            .str("category", "option")
            .str("baseCoin", &query.underlying);
        let rest = Rest::get("/v5/market/tickers", params).streamed();
        let url = format!("{}{}?{}", self.paths.rest, rest.path, rest.payload);
        let pending = Pending {
            category: self.category,
            rest,
            then: Then::Options(Request::Chain {
                context: context.clone(),
                query,
                quotes: Vec::new(),
                invalid: false,
            }),
            read: None,
        };
        match cx.request_managed_with_headers(
            context.clone(),
            None,
            Method::Get,
            url,
            &[("X-Referer", sign::BROKER_ID)],
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(MARKETS_PATH)),
            Err((_, error)) => {
                let _ = cx.fail_context(&context, error.venue_error());
            }
        }
    }

    /// Answered from the catalogue under the command's own authority.
    pub(crate) fn fetch_option_expiries(&self, cx: &mut Cx<'_, Pending>, underlying: &str) {
        let Ok(context) = cx.admit_http_command(entry_policy(Operation::Read)) else {
            return;
        };
        if !self.options_enabled(cx, &context) {
            return;
        }
        let mut expiries: Vec<f64> = cx
            .store
            .derivative_products()
            .into_iter()
            .flatten()
            .filter(|row| {
                row.underlying == underlying
                    && matches!(row.kind, ProductKind::Call | ProductKind::Put)
            })
            .filter_map(|row| row.expiry)
            .collect();
        expiries.sort_by(f64::total_cmp);
        expiries.dedup();
        let _ = cx.done_context(
            &context,
            serde_json::to_string(&expiries).unwrap_or_default(),
        );
    }

    /// The GET credential: the read key when configured, else the primary.
    /// Like trading, Greeks need the account's primary key at all.
    pub(crate) fn greeks_credentials(&self) -> Option<(CredentialSlot, sign::Credentials)> {
        self.select_key(crate::KeyUse::AccountRead)
            .map(|(selection, identity)| (selection.slot(), identity.clone()))
    }

    pub(crate) fn fetch_account_greeks(&mut self, cx: &mut Cx<'_, Pending>) {
        let identity = self.greeks_credentials();
        // A read may be repeated once after a native signature refusal.
        let policy = Policy {
            operation: Operation::Read,
            credential: identity.as_ref().map(|(slot, identity)| Credential {
                slot: *slot,
                reference: identity.credential.clone(),
            }),
            timeout_ms: read_policy().timeout_ms,
            pacing: None,
            retry: RetryPolicy::ReadFailure,
        };
        let Ok(context) = cx.admit_http_command(policy) else {
            return;
        };
        if !self.options_enabled(cx, &context) {
            return;
        }
        if identity.is_none() {
            let refusal = refused(
                "Bybit account Greeks need an API key",
                ErrorKind::AuthInvalid,
            );
            let _ = cx.fail_context(&context, refusal);
            return;
        }
        self.select_category(Category::Option);
        self.send_greeks(cx, context, false);
    }

    fn send_greeks(&mut self, cx: &mut Cx<'_, Pending>, context: Context, resent: bool) {
        let Some((_, credentials)) = self.greeks_credentials() else {
            let _ = cx.fail_context(
                &context,
                refused(
                    "Bybit account Greeks need an API key",
                    ErrorKind::AuthInvalid,
                ),
            );
            return;
        };
        let rest = Rest::get("/v5/asset/coin-greeks", Query::new())
            .streamed()
            .signed();
        let url = format!("{}{}", self.paths.rest, rest.path);
        let signing = sign::Http::new(credentials, rest.payload.clone(), self.recv_window)
            .with_header_policy(self.signing_policy.header_policy());
        let pending = Pending {
            category: self.category,
            rest,
            then: Then::Options(Request::Greeks {
                context: context.clone(),
                rows: Vec::new(),
                invalid: false,
                resent,
            }),
            read: None,
        };
        match cx.request_signed_managed(
            context.clone(),
            None,
            Method::Get,
            url,
            None,
            Box::new(signing),
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(MARKETS_PATH)),
            Err((_, error)) => {
                let _ = cx.fail_context(&context, error.venue_error());
            }
        }
    }

    /// A delivered answer, or the engine's typed failure for this read.
    pub(crate) fn on_options_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Request,
        response: Result<Response<'_>, VenueError>,
    ) {
        let context = read.context().clone();
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                let _ = cx.fail_context(&context, error);
                return;
            }
        };
        // Only this exact live delivery may answer the command.
        if cx.validate_read_http_delivery(&context, &context).is_err() {
            return;
        }
        let invalid = match &read {
            Request::Chain { invalid, .. } | Request::Greeks { invalid, .. } => *invalid,
        };
        if invalid {
            let _ = cx.fail_context(&context, malformed());
            return;
        }
        if let Some((code, message)) = venue_error(&response) {
            // Bybit's own clock is evidence; read it before sanitizing.
            let server = crate::trading::ordinary::server_time(&message);
            let message = crate::signing_rejection(code, &message).unwrap_or(message);
            let native_code = serde_json::from_str::<Status>(response.body)
                .ok()
                .and_then(|status| status.ret_code)
                .is_some()
                .then_some(code);
            let native = native_code.map(|code| Native {
                code: NativeCode::Int(code),
                message: &message,
            });
            let failure = classify::read(
                &BYBIT,
                Released::Response(&response),
                native.as_ref(),
                format!("Bybit options: {message}"),
            );
            if failure.retry == Some(ReadRetry::NativeResend)
                && matches!(read, Request::Greeks { resent: false, .. })
                && let Ok(retry) = cx.http_retry_context(&context, RetryEvidence::ReadFailure)
            {
                if let Some(server) = server {
                    cx.observe_protocol_time(server);
                }
                self.send_greeks(cx, retry, true);
                return;
            }
            let _ = cx.fail_context(&context, failure.error);
            return;
        }
        if !crate::account::valid_snapshot_list(response.body, "") {
            let _ = cx.fail_context(&context, malformed());
            return;
        }
        let result = match read {
            Request::Greeks { rows, .. } => serde_json::to_string(&rows),
            Request::Chain { query, quotes, .. } => {
                let mut rows: Vec<_> = cx
                    .store
                    .derivative_products()
                    .into_iter()
                    .flatten()
                    .filter(|row| matches(row, &query))
                    .filter_map(|product| {
                        let market = cx.store.markets().get(product.market as usize)?;
                        let quote = quotes.iter().find(|quote| quote.symbol == market.symbol)?;
                        Some(ChainRow {
                            market: OptionMarket {
                                market,
                                option: Terms {
                                    underlying: &product.underlying,
                                    expiry: product.expiry?,
                                    strike: product.strike?,
                                    right: if product.kind == ProductKind::Call {
                                        "call"
                                    } else {
                                        "put"
                                    },
                                    style: "european",
                                    settlement: "cash",
                                    premium_asset: product
                                        .premium_asset
                                        .as_deref()
                                        .unwrap_or(&market.quote),
                                    settle_asset: product
                                        .settlement_asset
                                        .as_deref()
                                        .unwrap_or(&market.quote),
                                },
                            },
                            quote,
                        })
                    })
                    .collect();
                rows.sort_by(|left, right| {
                    left.market
                        .option
                        .expiry
                        .total_cmp(&right.market.option.expiry)
                        .then_with(|| {
                            left.market
                                .option
                                .strike
                                .total_cmp(&right.market.option.strike)
                        })
                        .then_with(|| left.market.option.right.cmp(right.market.option.right))
                });
                serde_json::to_string(&rows)
            }
        };
        let _ = match result {
            Ok(data) => cx.done_context(&context, data),
            Err(error) => cx.fail_context(
                &context,
                VenueError::new(error.to_string()).kind(ErrorKind::Internal),
            ),
        };
    }
}
