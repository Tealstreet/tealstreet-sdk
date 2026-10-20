//! Native option terms, quote queries and account Greeks.
use crate::sign::Query;
use crate::wire::*;
use crate::{BybitLinear, Category, Pending, Rest, Then, venue_error};
use safe_cex_engine::model::{
    DerivativeProduct, Market, OptionChainQuery, OptionGreeks, OptionRight, ProductKind,
};
use safe_cex_engine::{CommandId, Cx, Response};
use serde::{Deserialize, Serialize};

type Opt<'a> = Option<Str<'a>>;

pub(crate) const GREEK_UNITS: &str = "Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day";

pub enum Request {
    Chain {
        command: CommandId,
        query: OptionChainQuery,
        quotes: Vec<Quote>,
        invalid: bool,
    },
    Greeks {
        command: CommandId,
        rows: Vec<AccountGreeks>,
        invalid: bool,
    },
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
    fn options_ready(&self, cx: &mut Cx<'_, Pending>, command: CommandId) -> bool {
        if self.category_enabled(Category::Option) {
            true
        } else {
            cx.done(
                command,
                Err("Options are not enabled on this Bybit account".into()),
            );
            false
        }
    }

    pub(crate) fn fetch_option_chain(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: OptionChainQuery,
    ) {
        if !self.options_ready(cx, command) {
            return;
        }
        if query.underlying.is_empty()
            || [query.expiry, query.strike_min, query.strike_max]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite() || value < 0.0)
            || matches!((query.strike_min, query.strike_max), (Some(min), Some(max)) if min > max)
        {
            cx.done(command, Err("Invalid Bybit option chain query".into()));
            return;
        }
        self.select_category(Category::Option);
        let params = Query::new()
            .str("category", "option")
            .str("baseCoin", &query.underlying);
        self.send(
            cx,
            Rest::get("/v5/market/tickers", params).streamed(),
            Then::Options(Request::Chain {
                command,
                query,
                quotes: Vec::new(),
                invalid: false,
            }),
        );
    }

    pub(crate) fn fetch_option_expiries(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        underlying: &str,
    ) {
        if !self.options_ready(cx, command) {
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
        cx.done(
            command,
            Ok(serde_json::to_string(&expiries).unwrap_or_default()),
        );
    }

    pub(crate) fn fetch_account_greeks(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        if !self.options_ready(cx, command) {
            return;
        }
        if self.credentials.is_none() {
            cx.done(command, Err("Bybit account Greeks need an API key".into()));
            return;
        }
        self.select_category(Category::Option);
        self.send(
            cx,
            Rest::get("/v5/asset/coin-greeks", Query::new())
                .streamed()
                .signed(),
            Then::Options(Request::Greeks {
                command,
                rows: Vec::new(),
                invalid: false,
            }),
        );
    }

    pub(crate) fn on_options_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let command = match &request {
            Request::Chain { command, .. } | Request::Greeks { command, .. } => *command,
        };
        let invalid = match &request {
            Request::Chain { invalid, .. } | Request::Greeks { invalid, .. } => *invalid,
        };
        if invalid {
            cx.done(command, Err("Malformed Bybit option response".into()));
            return;
        }
        let outcome = response
            .map_err(ToString::to_string)
            .and_then(|response| match venue_error(&response) {
                Some((_, error)) => Err(error),
                None if crate::account::valid_snapshot_list(response.body, "") => Ok(()),
                None => Err("Malformed Bybit option response".into()),
            });
        if let Err(error) = outcome {
            cx.done(command, Err(error.into()));
            return;
        }
        let result = match request {
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
        cx.done(
            command,
            result
                .map_err(|error| error.to_string())
                .map_err(Into::into),
        );
    }
}
