//! Caller-owned option chain observations and account Greeks.
use crate::{BinanceFutures, Pending, Query, reads};
use safe_cex_engine::credential_selection::CredentialRole;
use safe_cex_engine::model::{Market, OptionChainQuery, OptionGreeks, OptionRight, ProductKind};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Command, Cx, ErrorKind, Response, VenueError};
use serde::{Deserialize, Serialize};

const GREEK_UNITS: &str = "Black-Scholes per 1 unit of the underlying, in the premium asset: vega per 1 vol point, theta per calendar day";
const ACCOUNT_UNITS: &str = "As /eapi/v1/marginAccount reports them; scale not measured";
pub struct Request {
    root: Context,
    leg: Context,
    stage: Stage,
}
enum Stage {
    Tickers(OptionChainQuery),
    Marks(OptionChainQuery, Vec<Ticker>),
    Greeks,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Ticker {
    symbol: String,
    bid_price: String,
    ask_price: String,
    exercise_price: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mark {
    symbol: String,
    mark_price: String,
    #[serde(rename = "markIV")]
    mark_iv: String,
    #[serde(rename = "bidIV")]
    bid_iv: String,
    #[serde(rename = "askIV")]
    ask_iv: String,
    delta: Option<String>,
    gamma: Option<String>,
    vega: Option<String>,
    theta: Option<String>,
}
#[derive(Deserialize)]
struct Account {
    greek: Vec<Greek>,
}
#[derive(Deserialize)]
struct Greek {
    underlying: String,
    delta: String,
    gamma: String,
    vega: String,
    theta: String,
}
#[derive(Serialize)]
struct AccountGreeks {
    underlying: String,
    #[serde(flatten)]
    greeks: OptionGreeks,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Quote {
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
    quote: Quote,
}
fn error(message: &str) -> VenueError {
    VenueError::new(message).kind(ErrorKind::InvalidInput)
}
fn number(value: &str) -> Result<f64, VenueError> {
    value
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| error("Malformed option quote or Greeks"))
}
fn encode(value: &impl Serialize) -> Result<String, VenueError> {
    serde_json::to_string(value).map_err(|_| error("Option result encoding failed"))
}

impl BinanceFutures {
    pub(crate) fn options_product_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        match command {
            Command::FetchOptionExpiries { underlying, .. } => {
                let Ok(root) = Self::read_context(cx, None) else {
                    return Ok(());
                };
                let mut expiries: Vec<f64> = cx
                    .store
                    .derivative_products()
                    .into_iter()
                    .flatten()
                    .filter(|p| p.underlying == underlying)
                    .filter_map(|p| p.expiry)
                    .collect();
                expiries.sort_by(f64::total_cmp);
                expiries.dedup();
                match encode(&expiries) {
                    Ok(result) => {
                        let _ = cx.done_context(&root, result);
                    }
                    Err(error) => {
                        let _ = cx.fail_context(&root, error);
                    }
                }
            }
            Command::FetchOptionChain { query, .. } => {
                let Ok(root) = Self::read_context(cx, None) else {
                    return Ok(());
                };
                if query.underlying.is_empty()
                    || [query.expiry, query.strike_min, query.strike_max]
                        .into_iter()
                        .flatten()
                        .any(|v| !v.is_finite() || v < 0.)
                    || matches!((query.strike_min,query.strike_max),(Some(a),Some(b)) if a>b)
                {
                    let _ = cx.fail_context(&root, error("Invalid option chain query"));
                    return Ok(());
                }
                self.issue_option_product(cx, root, Stage::Tickers(query));
            }
            Command::FetchAccountGreeks { .. } => {
                if !self.credentials.has_primary() {
                    cx.refuse_command(
                        safe_cex_engine::request_context::Operation::Read,
                        error("Binance Options private credentials are required"),
                    );
                    return Ok(());
                }
                let Ok((selection, id)) = self
                    .credentials
                    .select(CredentialRole::Read { scope: None })
                else {
                    return Ok(());
                };
                let Ok(root) =
                    Self::read_context(cx, Some(selection.credential(id.credential.clone())))
                else {
                    return Ok(());
                };
                self.issue_option_product(cx, root, Stage::Greeks);
            }
            command => return Err(command),
        }
        Ok(())
    }
    fn issue_option_product(&mut self, cx: &mut Cx<'_, Pending>, root: Context, stage: Stage) {
        let selected = if matches!(stage, Stage::Greeks) {
            self.credentials
                .select(CredentialRole::Read { scope: None })
                .ok()
                .map(|(selection, id)| (selection.credential(id.credential.clone()), id.clone()))
        } else {
            None
        };
        let credential = selected.as_ref().map(|(c, _)| c.clone());
        let leg = match cx.http_leg_context(&root, reads::policy(credential)) {
            Ok(leg) => leg,
            Err(e) => {
                let _ = cx.fail_context(&root, e.venue_error());
                return;
            }
        };
        let path = match &stage {
            Stage::Tickers(_) => self.paths.daily,
            Stage::Marks(..) => self.paths.premium_index,
            Stage::Greeks => self.paths.balance,
        };
        self.managed_read(
            cx,
            leg.clone(),
            reads::Read {
                url: format!("{}{path}", self.urls.rest),
                credentials: selected.map(|(_, id)| id),
                query: Query::new(),
                stream: None,
            },
            Pending::OptionsProduct(Box::new(Request { root, leg, stage })),
        );
    }
    pub(crate) fn options_product_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: VenueError,
    ) {
        let _ = cx.finish_http_context(&request.leg);
        let _ = cx.fail_context(&request.root, error);
    }
    pub(crate) fn options_product_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        if cx
            .validate_read_http_delivery(&request.root, &request.leg)
            .is_err()
        {
            let _ = cx.finish_http_context(&request.leg);
            return;
        }
        if let Some((_, message)) = crate::account::venue_error(&response) {
            self.options_product_failed(cx, request, error(&message));
            return;
        }
        let result: Result<Option<String>, VenueError> = match &request.stage {
            Stage::Tickers(_) => serde_json::from_str::<Vec<Ticker>>(response.body)
                .map(|_| None)
                .map_err(|_| error("Malformed option ticker response")),
            Stage::Marks(query, tickers) => self
                .option_chain_json(cx, query, tickers, response.body)
                .map(Some),
            Stage::Greeks => serde_json::from_str::<Account>(response.body)
                .map_err(|_| error("Malformed account Greeks"))
                .and_then(|account| {
                    let rows = account
                        .greek
                        .into_iter()
                        .map(|g| {
                            let underlying = cx
                                .store
                                .derivative_products()
                                .into_iter()
                                .flatten()
                                .find(|p| format!("{}USDT", p.underlying) == g.underlying)
                                .map_or(g.underlying.clone(), |p| p.underlying.clone());
                            Ok(AccountGreeks {
                                underlying,
                                greeks: OptionGreeks {
                                    delta: number(&g.delta)?,
                                    gamma: number(&g.gamma)?,
                                    vega: number(&g.vega)?,
                                    theta: number(&g.theta)?,
                                    units: ACCOUNT_UNITS.into(),
                                },
                            })
                        })
                        .collect::<Result<Vec<_>, VenueError>>()?;
                    encode(&rows).map(Some)
                }),
        };
        let _ = cx.finish_http_context(&request.leg);
        match result {
            Err(error) => {
                let _ = cx.fail_context(&request.root, error);
            }
            Ok(Some(result)) => {
                let _ = cx.done_context(&request.root, result);
            }
            Ok(None) => {
                if let Stage::Tickers(query) = request.stage {
                    let tickers =
                        serde_json::from_str(response.body).expect("validated ticker response");
                    self.issue_option_product(cx, request.root, Stage::Marks(query, tickers));
                }
            }
        }
    }
    fn option_chain_json(
        &self,
        cx: &Cx<'_, Pending>,
        query: &OptionChainQuery,
        tickers: &[Ticker],
        body: &str,
    ) -> Result<String, VenueError> {
        let marks: Vec<Mark> =
            serde_json::from_str(body).map_err(|_| error("Malformed option marks"))?;
        let mut rows = Vec::new();
        for product in cx.store.derivative_products().into_iter().flatten() {
            if product.underlying != query.underlying
                || query.expiry.is_some_and(|v| Some(v) != product.expiry)
                || query
                    .strike_min
                    .is_some_and(|v| product.strike.is_none_or(|s| s < v))
                || query
                    .strike_max
                    .is_some_and(|v| product.strike.is_none_or(|s| s > v))
            {
                continue;
            }
            let right = match product.kind {
                ProductKind::Call => "call",
                ProductKind::Put => "put",
                _ => continue,
            };
            if query.right.is_some_and(|r| {
                matches!(
                    (r, product.kind),
                    (OptionRight::Call, ProductKind::Put) | (OptionRight::Put, ProductKind::Call)
                )
            }) {
                continue;
            }
            let Some(market) = cx.store.markets().get(product.market as usize) else {
                return Err(error("Option catalogue changed"));
            };
            let Some(mark) = marks.iter().find(|m| m.symbol == market.symbol) else {
                continue;
            };
            let ticker = tickers.iter().find(|t| t.symbol == market.symbol);
            let greeks = match (&mark.delta, &mark.gamma, &mark.vega, &mark.theta) {
                (None, None, None, None) => None,
                (Some(d), Some(g), Some(v), Some(t)) => Some(OptionGreeks {
                    delta: number(d)?,
                    gamma: number(g)?,
                    vega: number(v)?,
                    theta: number(t)?,
                    units: GREEK_UNITS.into(),
                }),
                _ => return Err(error("Incomplete option Greeks")),
            };
            let quote = Quote {
                symbol: market.symbol.clone(),
                bid: ticker
                    .map(|t| number(&t.bid_price))
                    .transpose()?
                    .unwrap_or(0.),
                ask: ticker
                    .map(|t| number(&t.ask_price))
                    .transpose()?
                    .unwrap_or(0.),
                underlying_price: ticker
                    .map(|t| number(&t.exercise_price))
                    .transpose()?
                    .unwrap_or(0.),
                mark: number(&mark.mark_price)?,
                mark_iv: number(&mark.mark_iv)?.max(0.),
                bid_iv: number(&mark.bid_iv)?.max(0.),
                ask_iv: number(&mark.ask_iv)?.max(0.),
                greeks,
            };
            rows.push(ChainRow {
                market: OptionMarket {
                    market,
                    option: Terms {
                        underlying: &product.underlying,
                        expiry: product
                            .expiry
                            .ok_or_else(|| error("Missing option expiry"))?,
                        strike: product
                            .strike
                            .ok_or_else(|| error("Missing option strike"))?,
                        right,
                        style: "european",
                        settlement: "cash",
                        premium_asset: product.premium_asset.as_deref().unwrap_or(&market.quote),
                        settle_asset: product.settlement_asset.as_deref().unwrap_or(&market.quote),
                    },
                },
                quote,
            });
        }
        encode(&rows)
    }
}
