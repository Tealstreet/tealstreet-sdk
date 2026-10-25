//! Independent native snapshot observations, separate from topic handoff.
use crate::{Pending, Task, WeexFutures, paths, rate, wire::*};
use safe_cex_engine::Venue;
use safe_cex_engine::request_context::{Context, Egress, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{Cx, Method, Response};

impl WeexFutures {
    pub(crate) fn snapshot(&mut self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        if !cx.now.is_finite() {
            return;
        }
        let Some(state) = self.books.get_mut(symbol) else {
            return;
        };
        if state.fetching {
            return;
        }
        if cx.now < self.paused_until {
            state.retry = Some(cx.set_timer(self.paused_until - cx.now));
            return;
        }
        self.next_generation += 1;
        state.generation = self.next_generation;
        let generation = state.generation;
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
            Err(error) => {
                self.depth_failed(cx, symbol, generation, None, &error.to_string());
                return;
            }
        };
        state.fetching = true;
        state.context = Some(context.clone());
        let pending = Pending {
            task: Task::Depth {
                symbol: symbol.into(),
                generation,
                context: Some(context.clone()),
            },
            invalid: false,
            skipped: Vec::new(),
            seed_generation: self.seed_generation,
        };
        if let Err((pending, error)) = cx.request_managed(
            context,
            None,
            Method::Get,
            format!("{}{}?limit=200&symbol={symbol}", paths::REST, paths::DEPTH),
            None,
            pending,
        ) {
            self.on_request_failed(cx, pending, &error.to_string());
        }
    }
    fn retire_depth(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        generation: u32,
        context: Option<Context>,
    ) -> bool {
        let owned = context.is_some_and(|context| cx.finish_http_context(&context).is_ok());
        if let Some(book) = self.books.get_mut(symbol)
            && book.generation == generation
        {
            book.context = None;
            book.fetching = false;
        }
        owned
    }
    pub(crate) fn depth_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        generation: u32,
        context: Option<Context>,
        error: &str,
    ) {
        let admitted = context.is_some();
        let owned = self.retire_depth(cx, symbol, generation, context);
        if !cx.now.is_finite()
            || admitted && !owned
            || !self
                .books
                .get(symbol)
                .is_some_and(|b| b.generation == generation)
        {
            return;
        }
        cx.notice(
            safe_cex_engine::Level::Warn,
            safe_cex_engine::log::code::REQUEST_FAILED,
            format!("WEEX: {error}"),
        );
        self.book_failed(cx, symbol);
    }
    pub(crate) fn depth_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        symbol: &str,
        generation: u32,
        context: Option<Context>,
        response: Response<'_>,
    ) {
        let current = self
            .books
            .get(symbol)
            .is_some_and(|b| b.generation == generation);
        if !current
            || !context
                .as_ref()
                .is_some_and(|c| cx.validate_background_read_http_delivery(c).is_ok())
        {
            self.retire_depth(cx, symbol, generation, context);
            return;
        }
        if response.status == 429 {
            self.paused_until = self
                .paused_until
                .max(cx.now + rate::delay(response, cx.now));
        }
        if response.status >= 400 {
            self.depth_failed(
                cx,
                symbol,
                generation,
                context,
                &format!("HTTP {}: {}", response.status, response.body),
            );
            return;
        }
        let Ok(row) = serde_json::from_str::<Depth>(response.body) else {
            self.retire_depth(cx, symbol, generation, context);
            self.book_failed(cx, symbol);
            return;
        };
        let b = self.books.get_mut(symbol).expect("current book");
        b.book.clear();
        for (side, levels) in [
            (safe_cex_engine::book::BookSide::Bids, row.bids),
            (safe_cex_engine::book::BookSide::Asks, row.asks),
        ] {
            for Level(p, a) in levels {
                if p > 0.0 && a >= 0.0 {
                    b.book.apply(side, p, a);
                }
            }
        }
        b.last = Some(row.last_update_id);
        b.fetching = false;
        for delta in std::mem::take(&mut b.buffer) {
            if !crate::apply(b, &delta) {
                self.retire_depth(cx, symbol, generation, context);
                self.book_failed(cx, symbol);
                return;
            }
        }
        b.failures = 0;
        cx.book_changed(symbol);
        self.retire_depth(cx, symbol, generation, context);
        self.reconcile_topics(cx);
    }
}
