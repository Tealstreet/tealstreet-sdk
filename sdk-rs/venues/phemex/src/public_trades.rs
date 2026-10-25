//! One unsigned recent-trades response under original command Read authority.
use crate::history::{Deferred, Trade, decode_public_trade};
use crate::{Pending, PhemexPerpetuals};
use safe_cex_engine::history::{ReadCompletion, ReadHistoryRows};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::{CommandId, Cx, Method, Response, Venue, VenueError};
use serde::Deserialize;

pub struct Read {
    rows: ReadHistoryRows<Trade>,
    symbol: String,
    native_symbol: String,
    limit: usize,
    count: usize,
    invalid: bool,
}

fn encode(rows: &[Trade]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}

impl Read {
    pub(super) fn valid_delivery(&self, cx: &Cx<'_, Pending>) -> bool {
        self.rows.validate_delivery(cx, self.rows.root()).is_ok()
    }

    pub(super) fn item(&mut self, cx: &Cx<'_, Pending>, item: &str) {
        if self.invalid || !self.valid_delivery(cx) {
            return;
        }
        self.count += 1;
        if self.count > 1000 {
            self.invalid = true;
            return;
        }
        let Some(row) = decode_public_trade(item, &self.symbol, self.count - 1) else {
            self.invalid = true;
            return;
        };
        let root = self.rows.root().clone();
        if let Ok(mut page) = self.rows.admit(cx, &root) {
            page.stage(row);
        }
    }
}

impl PhemexPerpetuals {
    pub(super) fn fetch_public_trades(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        limit: Option<u32>,
    ) {
        let Ok(root) = cx.admit_http_command(Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 20_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }) else {
            return;
        };
        let error = if cx.store.market(&symbol).is_none() {
            Some("Phemex history requires a known symbol")
        } else if self.history.active.len() >= 16 || self.history.active.contains(&command) {
            Some("Too many active Phemex history queries")
        } else if !self
            .native_symbol(&symbol)
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            Some("Unsupported Phemex native history symbol")
        } else {
            None
        };
        if let Some(error) = error {
            let _ = cx.fail_context(&root, error);
            return;
        }
        let completion = match ReadCompletion::from_context(cx, command, root.clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(&root, error.venue_error());
                return;
            }
        };
        let read = Box::new(Read {
            rows: ReadHistoryRows::new(completion, encode),
            native_symbol: self.native_symbol(&symbol).into(),
            symbol,
            limit: limit.unwrap_or(100).clamp(1, 200) as usize,
            count: 0,
            invalid: false,
        });
        self.history.active.insert(command);
        self.send_public_trades(cx, read);
    }

    pub(super) fn send_public_trades(&mut self, cx: &mut Cx<'_, Pending>, read: Box<Read>) {
        if let Err(error) = cx.validate_http_context(read.rows.root()) {
            self.public_trades_failed(cx, *read, error.venue_error());
            return;
        }
        if cx.now < self.paused_until {
            let timer = cx.set_timer(self.paused_until - cx.now);
            self.history
                .deferred
                .push((timer, Deferred::PublicTrades(read)));
            return;
        }
        let root = read.rows.root().clone();
        let url = format!("{}/md/v2/trade?symbol={}", self.rest(), read.native_symbol);
        match cx.request_managed(
            root,
            None,
            Method::Get,
            url,
            None,
            Pending::PublicTrades(read),
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(&["result", "trades_p"])),
            Err((pending, error)) => self.on_sign_error(cx, pending, error.venue_error()),
        }
    }

    pub(super) fn public_trades_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut read: Read,
        response: Response<'_>,
    ) {
        if !(200..300).contains(&response.status) {
            self.public_trades_failed(
                cx,
                read,
                VenueError::new(format!("Phemex history HTTP {}", response.status)),
            );
            return;
        }
        #[derive(Deserialize)]
        struct Envelope<'a> {
            #[serde(borrow)]
            result: Recent<'a>,
            error: Option<&'a serde_json::value::RawValue>,
        }
        #[derive(Deserialize)]
        struct Recent<'a> {
            symbol: &'a str,
            #[serde(borrow)]
            trades_p: &'a serde_json::value::RawValue,
        }
        let parsed = serde_json::from_str::<Envelope<'_>>(response.body)
            .ok()
            .filter(|body| body.error.is_none() && body.result.symbol == read.native_symbol)
            .is_some_and(|body| {
                safe_cex_engine::json::each::<&serde_json::value::RawValue>(
                    body.result.trades_p.get(),
                    |row| read.item(cx, row.get()),
                )
                .is_ok()
            });
        if !parsed || read.invalid {
            self.public_trades_failed(
                cx,
                read,
                VenueError::new("Malformed or oversized Phemex history response"),
            );
            return;
        }
        let root = read.rows.root().clone();
        let Ok(mut page) = read.rows.admit(cx, &root) else {
            return;
        };
        page.accept_page();
        let command = read.rows.command();
        let limit = read.limit;
        let result = read.rows.finish_with(cx, Ok(()), |mut rows| {
            rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
            rows.truncate(limit);
            encode(&rows)
        });
        if result.is_ok() {
            self.history.active.remove(&command);
        }
    }

    pub(super) fn public_trades_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        error: VenueError,
    ) {
        let command = read.rows.command();
        if read.rows.reject(cx, error).is_ok() {
            self.history.active.remove(&command);
        }
    }
}
