//! One genuine public MARK receipt before admitting a protective mutation.
use super::*;
use crate::wire::Stats;
use safe_cex_engine::{Store, json::ArrayAt};

pub struct ReferenceRead {
    preflight: Preflight,
    context: Context,
    symbol: String,
    market: u32,
    quote: Option<(f64, f64)>,
    store_mark: Option<f64>,
    invalid: bool,
}
impl ReferenceRead {
    pub(crate) fn item(&mut self, cx: &Cx<'_, Pending>, item: &str) {
        if cx
            .validate_preflight_delivery(&self.preflight, &self.context)
            .is_err()
        {
            return;
        }
        match serde_json::from_str::<Stats<'_>>(item) {
            Ok(row) if row.market_id == Some(f64::from(self.market)) => {
                if self.quote.is_some() || (!row.symbol.is_empty() && row.symbol != self.symbol) {
                    self.invalid = true;
                }
                match row.mark_price.filter(|p| p.is_finite() && *p > 0.0) {
                    Some(mark) => {
                        self.quote = Some((mark, cx.now));
                        self.store_mark = cx.store.ticker(&self.symbol).map(|t| t.mark);
                    }
                    None => self.invalid = true,
                }
            }
            Ok(_) => {}
            Err(_) => self.invalid = true,
        }
    }
}

impl Lighter {
    pub(super) fn protective_needs_reference(
        &self,
        store: &Store,
        order: &Order,
        now: f64,
    ) -> bool {
        matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit)
            && !self
                .protection
                .marks
                .get(&order.symbol)
                .is_some_and(|(mark, at)| {
                    now.is_finite()
                        && now >= *at
                        && now - at <= 5_000.0
                        && store.ticker(&order.symbol).is_some_and(|t| t.mark == *mark)
                })
    }

    pub(super) fn trade_reference_start(&mut self, cx: &mut Cx<'_, Pending>) {
        let owner = self.trading.active.as_ref().unwrap().preflight.clone();
        let context = match cx.preflight_public_read_context(
            &owner,
            Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            },
            safe_cex_engine::request_context::plan::CallTerms {
                wait_ms: 0,
                timeout_ms: 20_000,
                attempts: 1,
                retry_window: safe_cex_engine::request_context::plan::RetryWindow::FreshAttempt,
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                self.trade_finish(cx, error.venue_error().outcome(Outcome::NotSent));
                return;
            }
        };
        let job = self
            .trading
            .active
            .as_mut()
            .expect("active protective preflight");
        job.reference_checked = true;
        job.live = Some(context.clone());
        let edit = job.protection.as_ref().expect("protective reference");
        let read = ReferenceRead {
            preflight: job.preflight.clone(),
            context: context.clone(),
            symbol: edit.replacement.symbol.clone(),
            market: edit.original.native.market_id,
            quote: None,
            store_mark: None,
            invalid: false,
        };
        self.trading.inflight = true;
        match cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}/api/v1/exchangeStats", self.host()),
            None,
            Pending::Trade(Request::Reference(Box::new(read))),
        ) {
            Ok(id) => cx.stream_response(
                id,
                ArrayAt::Envelope(&[
                    &["order_book_stats"],
                    &["data", "order_book_stats"],
                    &["result", "order_book_stats"],
                    &["tickers"],
                    &["data", "tickers"],
                    &["result", "tickers"],
                ]),
            ),
            Err((_, error)) => {
                self.trading.inflight = false;
                self.trade_finish(cx, error.venue_error().outcome(Outcome::NotSent));
            }
        }
    }

    fn reference_active(&self, read: &ReferenceRead) -> bool {
        self.trading.active.as_ref().is_some_and(|job| {
            job.preflight.owner() == read.preflight.owner()
                && job.live.as_ref() == Some(&read.context)
        })
    }

    pub(super) fn trade_reference_rejected(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: &ReferenceRead,
        error: VenueError,
    ) {
        if self.reference_active(read) {
            self.trading.inflight = false;
            self.trade_finish(
                cx,
                not_sent(format!(
                    "Lighter stop not changed: reference stale ({})",
                    error.message
                )),
            );
        }
    }

    pub(super) fn trade_reference_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: ReferenceRead,
        response: Response<'_>,
    ) {
        if !self.reference_active(&read)
            || cx
                .validate_preflight_delivery(&read.preflight, &read.context)
                .is_err()
        {
            return;
        }
        if let Err(error) = self.protective_account_veto() {
            self.trading.inflight = false;
            self.trade_finish(cx, error);
            return;
        }
        let accepted = (200..300).contains(&response.status)
            && serde_json::from_str::<Reply>(response.body)
                .is_ok_and(|r| r.code.as_ref().and_then(Number::integer) == Some(200));
        let Some((mark, at)) = read.quote.filter(|(_, at)| {
            accepted
                && !read.invalid
                && cx.now.is_finite()
                && (0.0..=5_000.0).contains(&(cx.now - at))
        }) else {
            self.trade_reference_rejected(
                cx,
                &read,
                not_sent("missing, failed, invalid or delayed public MARK"),
            );
            return;
        };
        let replacement = self
            .trading
            .active
            .as_ref()
            .unwrap()
            .protection
            .as_ref()
            .unwrap()
            .replacement
            .clone();
        let newer_native = self
            .protection
            .marks
            .get(&read.symbol)
            .is_some_and(|(_, current_at)| *current_at > at);
        if !newer_native {
            let current = cx.store.ticker(&read.symbol).map(|t| t.mark);
            if current != read.store_mark && current != Some(mark) {
                self.trade_reference_rejected(
                    cx,
                    &read,
                    not_sent("MARK changed after the public row"),
                );
                return;
            }
            cx.store.update_ticker(&read.symbol, |t| t.mark = mark);
            self.observe_mark(&read.symbol, Some(mark), at);
        }
        if let Err(error) = self.protective_reference(cx.store, &replacement, cx.now) {
            self.trading.inflight = false;
            self.trade_finish(cx, error);
            return;
        }
        if let Err(error) = cx.finish_preflight_call(&read.preflight, &read.context) {
            self.trade_reference_rejected(cx, &read, error.venue_error());
            return;
        }
        self.trading.active.as_mut().unwrap().live = None;
        self.trading.inflight = false;
        self.trade_kick(cx);
    }
}
