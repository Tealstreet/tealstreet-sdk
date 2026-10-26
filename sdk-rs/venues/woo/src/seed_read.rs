//! Exact physical ownership for unsigned public seed observations.
use crate::wire::*;
use crate::{Pending, Woo, account, compact, keep_listed, reread_partial};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
use safe_cex_engine::request_context::{
    Context, Egress, Error, Operation, Policy, RetryPolicy, Scope,
};
use safe_cex_engine::{Cx, ErrorKind, Level, Method, Response, VenueError, num};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(crate) enum Kind {
    Markets,
    Tickers,
    Funding,
}
pub struct Read {
    context: Context,
    payload: Payload,
}
enum Payload {
    Markets {
        rows: Vec<Market>,
        ranges: HashMap<String, f64>,
        skipped: account::Skipped,
    },
    Tickers {
        rows: Vec<Ticker>,
        indexes: HashMap<String, u32>,
        skipped: account::Skipped,
    },
    Funding {
        rows: Vec<(String, f64, f64)>,
    },
}
impl Payload {
    fn kind(&self) -> Kind {
        match self {
            Self::Markets { .. } => Kind::Markets,
            Self::Tickers { .. } => Kind::Tickers,
            Self::Funding { .. } => Kind::Funding,
        }
    }
}
enum Followup {
    Markets {
        refresh: bool,
        retry: bool,
        private: bool,
    },
    Tickers,
    None,
}
impl Woo {
    pub(crate) fn seed_read(&mut self, cx: &mut Cx<'_, Pending>, kind: Kind) {
        if !cx.now.is_finite() {
            return;
        }
        let context = match cx.http_context_with_egress(
            Scope::Background,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            Egress::Primary,
        ) {
            Ok(context) => context,
            Err(reason) => {
                self.seed_failure_effects(
                    cx,
                    kind,
                    &reason.read_venue_error().message,
                    matches!(reason, Error::Capacity | Error::TimedOut),
                );
                return;
            }
        };
        let (path, payload) = match kind {
            Kind::Markets => (
                "/v1/public/info",
                Payload::Markets {
                    rows: vec![],
                    ranges: HashMap::new(),
                    skipped: Default::default(),
                },
            ),
            Kind::Tickers => (
                "/v1/public/futures",
                Payload::Tickers {
                    rows: vec![],
                    indexes: HashMap::new(),
                    skipped: Default::default(),
                },
            ),
            Kind::Funding => (
                "/v1/public/funding_rates",
                Payload::Funding { rows: vec![] },
            ),
        };
        let pending = Pending::Seed(Box::new(Read {
            context: context.clone(),
            payload,
        }));
        match cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}{path}", self.rest),
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Field("rows")),
            Err((Pending::Seed(read), reason)) => {
                self.seed_precommit_failed(cx, *read, reason, reason.read_venue_error())
            }
            Err(_) => unreachable!(),
        }
    }
    pub(crate) fn seed_item(&mut self, cx: &mut Cx<'_, Pending>, read: &mut Read, item: &str) {
        if cx
            .validate_background_read_http_delivery(&read.context)
            .is_err()
        {
            return;
        }
        match &mut read.payload {
            Payload::Markets {
                rows,
                ranges,
                skipped,
            } => {
                let raw = match serde_json::from_str::<Instrument<'_>>(item) {
                    Ok(raw) => raw,
                    Err(_) => {
                        match serde_json::from_str::<Symbol<'_>>(item) {
                            Ok(s) if !s.symbol.starts_with("PERP_") => (),
                            Ok(s) => {
                                skipped.add(Some(&s.symbol));
                                safe_cex_engine::startup::skipped_row(cx);
                            }
                            Err(_) => {
                                skipped.add(None);
                                safe_cex_engine::startup::skipped_row(cx);
                            }
                        }
                        return;
                    }
                };
                let Some((base, quote)) = raw
                    .symbol
                    .strip_prefix("PERP_")
                    .and_then(|s| s.split_once('_'))
                else {
                    return;
                };
                if raw.base_tick <= 0.0
                    || raw.quote_tick <= 0.0
                    || raw.base_min <= 0.0
                    || raw.base_max < raw.base_min
                {
                    skipped.add(Some(&raw.symbol));
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                }
                let symbol = compact(&raw.symbol);
                if let Some(range) = raw.price_range.filter(|v| *v > 0.0) {
                    ranges.insert(symbol.clone(), range);
                }
                rows.push(Market {
                    id: format!("{base}/{quote}:USDT"),
                    symbol,
                    pretty_symbol: None,
                    base: base.into(),
                    quote: quote.into(),
                    active: true,
                    precision: Precision {
                        amount: raw.base_tick,
                        price: raw.quote_tick,
                    },
                    limits: Limits {
                        amount: MinMax {
                            min: raw.base_min,
                            max: raw.base_max,
                        },
                        leverage: MinMax {
                            min: 1.0,
                            max: (1.0
                                / if raw.base_imr > 0.0 {
                                    raw.base_imr
                                } else {
                                    0.01
                                })
                            .floor(),
                        },
                        price: None,
                    },
                    contract_size: None,
                    is_expiring: None,
                    is_inverse: None,
                    is_spot: None,
                    is_perpetual: None,
                    is_read_only: None,
                    taker_fee: None,
                    maker_fee: None,
                });
            }
            Payload::Tickers {
                rows,
                indexes,
                skipped,
            } => {
                let raw = match serde_json::from_str::<Statistics<'_>>(item) {
                    Ok(raw) => raw,
                    Err(_) => {
                        match serde_json::from_str::<Symbol<'_>>(item) {
                            Ok(s) if cx.store.market(&compact(&s.symbol)).is_none() => (),
                            Ok(s) => {
                                skipped.add(Some(&s.symbol));
                                safe_cex_engine::startup::skipped_row(cx);
                            }
                            Err(_) => {
                                skipped.add(None);
                                safe_cex_engine::startup::skipped_row(cx);
                            }
                        }
                        return;
                    }
                };
                let symbol = compact(&raw.symbol);
                let Some(m) = cx.store.market(&symbol) else {
                    return;
                };
                if raw.close <= 0.0 {
                    skipped.add(Some(&raw.symbol));
                    safe_cex_engine::startup::skipped_row(cx);
                    return;
                }
                indexes.insert(raw.symbol.to_string(), rows.len() as u32);
                rows.push(Ticker {
                    id: m.id.clone(),
                    symbol,
                    bid: raw.close,
                    ask: raw.close,
                    last: raw.close,
                    mark: raw.mark_price,
                    index: raw.index_price.unwrap_or(f64::NAN),
                    percentage: if raw.open > 0.0 {
                        num::round_half_up((raw.close - raw.open) / raw.open * 10000.0) / 100.0
                    } else {
                        0.0
                    },
                    open_interest: raw.open_interest,
                    funding_rate: raw.last_funding_rate,
                    funding_time: raw.next_funding_time,
                    volume: raw.volume,
                    quote_volume: raw.amount,
                    mid_price: None,
                });
            }
            Payload::Funding { rows } => {
                if let Ok(raw) = serde_json::from_str::<Funding<'_>>(item)
                    && raw.next_funding_time > 0.0
                {
                    rows.push((
                        compact(&raw.symbol),
                        raw.est_funding_rate,
                        raw.next_funding_time,
                    ));
                }
            }
        }
    }
    pub(crate) fn seed_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        response: Response<'_>,
    ) {
        if !cx.now.is_finite() {
            let _ = cx.finish_http_context(&read.context);
            return;
        }
        if cx
            .validate_background_read_http_delivery(&read.context)
            .is_err()
        {
            return;
        }
        if !(200..300).contains(&response.status) || Envelope::checked(response.body).is_err() {
            self.seed_failed(cx, read, "WOO read rejected", true);
            return;
        }
        let Read { context, payload } = read;
        let kind = payload.kind();
        let followup = match payload {
            Payload::Markets {
                rows,
                ranges,
                skipped,
            } => {
                if let Some(text) = skipped.summary("markets") {
                    cx.notice(Level::Warn, code::ROW_SKIPPED, text);
                }
                if rows.is_empty() {
                    self.seed_retire_failed(cx, &context, kind, "Invalid WOO markets", true);
                    return;
                }
                let refresh = std::mem::replace(&mut self.catalogue_committed, true);
                let rows = if refresh {
                    let mut ranges = ranges;
                    for (symbol, range) in std::mem::take(&mut self.price_ranges) {
                        ranges.entry(symbol).or_insert(range);
                    }
                    self.price_ranges = ranges;
                    keep_listed(cx.store.markets(), rows)
                } else {
                    self.price_ranges = ranges;
                    rows
                };
                cx.store.set_markets(rows);
                self.skipped_markets = skipped
                    .ids()
                    .iter()
                    .filter(|id| cx.store.market(&compact(id)).is_none())
                    .cloned()
                    .collect();
                let before = self.malformed_rows;
                self.malformed_rows = self.skipped_markets.len() + usize::from(skipped.anonymous());
                let retry = reread_partial(
                    cx,
                    &mut self.reread_stalls,
                    refresh,
                    before,
                    self.malformed_rows,
                );
                let private = !self.options.public_data_only
                    && self
                        .options
                        .key
                        .as_deref()
                        .is_some_and(|key| !key.is_empty());
                if !refresh && !private {
                    cx.store.set_balance(Default::default());
                    cx.store.set_positions(Vec::new());
                    cx.store.set_orders(Vec::new());
                }
                Followup::Markets {
                    refresh,
                    retry,
                    private,
                }
            }
            Payload::Tickers {
                rows,
                indexes,
                skipped,
            } => {
                if let Some(text) = skipped.summary("tickers") {
                    cx.notice(Level::Warn, code::ROW_SKIPPED, text);
                }
                if rows.is_empty() {
                    self.seed_retire_failed(cx, &context, kind, "Invalid WOO tickers", true);
                    return;
                }
                self.ticker_indexes = indexes;
                cx.store.set_tickers(rows);
                Followup::Tickers
            }
            Payload::Funding { rows } => {
                for (symbol, rate, time) in rows {
                    cx.store.update_ticker(&symbol, |t| {
                        t.funding_rate = rate;
                        t.funding_time = time;
                    });
                }
                Followup::None
            }
        };
        if cx.finish_http_context(&context).is_err() {
            return;
        }
        match followup {
            Followup::Markets {
                refresh,
                retry,
                private,
            } => {
                if retry {
                    self.seed_backoff.failed(cx, false);
                } else {
                    self.seed_backoff.succeeded(cx, &false);
                }
                if !refresh {
                    if private {
                        self.account_start(cx);
                    }
                    self.seed(cx, true);
                }
            }
            Followup::Tickers => {
                self.seed_backoff.succeeded(cx, &true);
                self.account_refresh(cx);
                self.connect_public(cx);
                self.funding = Some(cx.set_timer(60_000.0));
            }
            Followup::None => (),
        }
    }
    pub(crate) fn seed_precommit_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        reason: Error,
        error: VenueError,
    ) {
        let recover = matches!(reason, Error::Capacity | Error::TimedOut)
            || (reason == Error::Invalid
                && matches!(
                    error.kind,
                    Some(ErrorKind::Network | ErrorKind::Timeout | ErrorKind::Internal)
                ));
        self.seed_failed(cx, read, &error.message, recover);
    }
    pub(crate) fn seed_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        error: &str,
        recover: bool,
    ) {
        self.seed_retire_failed(cx, &read.context, read.payload.kind(), error, recover);
    }
    fn seed_retire_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        context: &Context,
        kind: Kind,
        error: &str,
        recover: bool,
    ) {
        if cx.finish_http_context(context).is_ok() && cx.now.is_finite() {
            self.seed_failure_effects(cx, kind, error, recover);
        }
    }
    fn seed_failure_effects(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        kind: Kind,
        error: &str,
        recover: bool,
    ) {
        cx.notice(Level::Warn, code::REQUEST_FAILED, error);
        if recover {
            match kind {
                Kind::Markets => self.seed_backoff.failed(cx, false),
                Kind::Tickers => self.seed_backoff.failed(cx, true),
                Kind::Funding => (),
            }
        }
    }
}
