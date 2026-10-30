//! Paradex perpetuals on the sans-I/O engine.
mod account;
mod depth_query;
mod history;
pub mod hosted;
mod identity;
mod public;
mod sbe;
pub mod sign;
mod trading;
mod venue_state;
mod wire;

use safe_cex_engine::{Command, Cx, Response, SocketId, TimerId, Venue, book::Book};

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub paradex_address: Option<String>,
    pub subkey_address: Option<String>,
    /// Trusted local setup's identity; its Stark public key is sealed there.
    pub credentials: Option<safe_cex_engine::signing::Identity>,
    /// Setup derived the Stark key from an Ethereum key, so the account
    /// address is the one that key derives.
    pub ethereum_key: bool,
}
safe_cex_crypto::redacted_debug!(Options);

impl Options {
    fn private_enabled(&self) -> bool {
        !self.public_data_only
            && self.credentials.is_some()
            && self
                .paradex_address
                .as_deref()
                .is_some_and(|s| !s.is_empty())
            && self
                .subkey_address
                .as_deref()
                .is_some_and(|s| !s.is_empty())
    }
}

pub enum Pending {
    BookQuery(safe_cex_engine::request_context::Context),
    Public(public::Request),
    Account(account::Request),
    Trade(trading::Request),
    History(history::Request),
    PublicTrades(history::public_trades::Request),
    Identity(safe_cex_engine::request_context::Context),
    /// A FetchLeverage command Read and its symbol.
    Leverage(safe_cex_engine::request_context::Context, String),
}

pub struct Paradex {
    options: Options,
    public: public::Public,
    account: account::Account,
    trading: trading::Trading,
    history: history::History,
}

impl Paradex {
    pub fn new(options: Options) -> Self {
        Self {
            account: account::Account::new(options.clone()),
            trading: trading::Trading::default(),
            history: history::History,
            options,
            public: public::Public::default(),
        }
    }
    fn rest(&self) -> &'static str {
        if self.options.testnet {
            "https://api.testnet.paradex.trade/v1"
        } else {
            "https://api.prod.paradex.trade/v1"
        }
    }
    fn ws(&self) -> &'static str {
        if self.options.testnet {
            "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2"
        } else {
            "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false&sbeSchemaId=1&sbeSchemaVersion=2"
        }
    }
}

impl Venue for Paradex {
    type Pending = Pending;
    /// A host-answered provider binds its Stark public key before start; a
    /// trusted local provider keeps the key it sealed at setup.
    fn public_key_setup(
        &self,
    ) -> Option<(
        safe_cex_engine::AddressSetupSlots,
        safe_cex_engine::signing::Algorithm,
    )> {
        self.options.credentials.as_ref().map(|_| {
            (
                safe_cex_engine::AddressSetupSlots::Primary,
                safe_cex_engine::signing::Algorithm::StarkDigest,
            )
        })
    }
    /// Only `/auth` is credential evidence: a 401/403 is a definitive refusal,
    /// and a 2xx carrying a JWT is acceptance.
    fn account_auth_outcome(
        &self,
        pending: &Self::Pending,
        response: &Response<'_>,
        _now: f64,
        _store: &safe_cex_engine::Store,
    ) -> Option<safe_cex_engine::AuthOutcome> {
        if !matches!(pending, Pending::Account(account::Request::Auth { .. })) {
            return None;
        }
        match response.status {
            401 | 403 => Some(safe_cex_engine::AuthOutcome::Rejected),
            200..=299 => {
                account::auth_token(response.body).map(|_| safe_cex_engine::AuthOutcome::Accepted)
            }
            _ => None,
        }
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            position_history: safe_cex_engine::PositionHistory::FillsSymbol,
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            client_order_lookup: true,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            account: safe_cex_engine::AccountLookup::Query,
            order_book_query: true,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.public_start(cx);
    }
    fn refresh_settings(&mut self, cx: &mut Cx<'_, Pending>, _symbol: Option<&str>) {
        // Paradex reads every market's margin config at once.
        self.account.refresh_settings(cx);
    }
    fn export_venue_state(&self) -> Option<Vec<u8>> {
        Some(self.account.export_venue_state())
    }
    fn apply_venue_state(&mut self, cx: &mut Cx<'_, Pending>, state: &[u8]) {
        if let Err(error) = self.account.apply_venue_state(state) {
            cx.notice(
                safe_cex_engine::Level::Warn,
                safe_cex_engine::log::code::STREAM_MALFORMED,
                error,
            );
        }
    }
    fn on_frame_ingested(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.prune_native(cx.store);
    }
    // reconcile_uncertain starts an order read dispatched after the write.
    fn fence_uncertain_placements(&self) -> bool {
        true
    }
    // Private reads bind the account's JWT Session; the engine copies the Bearer
    // only at prepare and re-prepares when the JWT moves before release.
    fn session_bearer(&self, generation: u64) -> safe_cex_engine::request_context::SessionBearer {
        self.account.session_bearer(generation)
    }
    fn session_bearer_revision(&self, generation: u64) -> Option<u64> {
        self.account.session_bearer_revision(generation)
    }
    fn reconcile_uncertain(&mut self, cx: &mut Cx<'_, Pending>, _symbols: Option<&[String]>) {
        self.account.reconcile_orders(cx);
        self.failures(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.handles_socket(socket) {
            self.account.on_open(cx, socket);
        } else {
            self.public_open(cx, socket);
        }
        self.failures(cx);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if self.account.handles_socket(socket) {
            self.account.on_message(cx, socket, text);
        } else {
            self.public_message(cx, socket, text);
        }
        self.failures(cx);
    }
    fn on_binary(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, data: &[u8]) {
        if self.account.handles_socket(socket) {
            self.account.on_binary(cx, socket, data);
        } else {
            self.public_binary(cx, socket, data);
        }
        self.failures(cx);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if self.account.handles_socket(socket) {
            self.account.on_close(cx, socket);
        } else {
            self.public_close(cx, socket);
        }
        self.failures(cx);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Public(request) => self.public_item(cx, request, item),
            Pending::Account(request) => self.account.on_item(cx, request, item),
            Pending::History(request) => self.history.on_item(cx, request, item),
            Pending::PublicTrades(read) => history::public_trades::item(cx, read, item),
            Pending::Trade(_)
            | Pending::Identity(_)
            | Pending::Leverage(..)
            | Pending::BookQuery(_) => {}
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        match pending {
            Pending::BookQuery(context) => {
                self.book_query_response(cx, &context, response);
                return;
            }
            Pending::PublicTrades(read) => history::public_trades::response(cx, read, response),
            Pending::Identity(context) => self.identity_response(cx, context, response),
            Pending::Leverage(context, symbol) => {
                self.trading
                    .leverage_response(cx, &mut self.account, &context, &symbol, response)
            }
            Pending::Public(request) => {
                self.public_response(cx, request, response);
                if cx.store.loaded().markets {
                    self.account.start(cx);
                }
            }
            Pending::Account(request) => self.account.on_response(cx, request, response),
            Pending::Trade(request) => {
                self.trading
                    .on_response(cx, &mut self.account, request, response)
            }
            Pending::History(request) => {
                self.history
                    .on_response(cx, &mut self.account, request, response)
            }
        }
        self.failures(cx);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut pending: Pending,
        response: Response<'_>,
    ) {
        if let Pending::Public(request) = &mut pending
            && request.is_command_candle()
            && !request.command_candle_delivery(cx)
        {
            request.retire_command_candle(cx);
            return;
        }
        if let Pending::Public(request) = &mut pending
            && self.discard_stale_poll(cx, request)
        {
            return;
        }
        let error = success(response)
            .err()
            .unwrap_or_else(|| format!("Malformed Paradex list (HTTP {})", response.status));
        if let Pending::PublicTrades(read) = pending {
            history::public_trades::failed(cx, read, safe_cex_engine::VenueError::new(error));
        } else {
            self.on_request_failed(cx, pending, &error);
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        if let Pending::BookQuery(context) = pending {
            let _ = cx.fail_context(
                &context,
                depth_query::failure(error, safe_cex_engine::ErrorKind::Network),
            );
            return;
        }
        self.dispatch_failed(cx, pending, error);
        self.failures(cx);
    }
    fn on_sign_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Trade(request) => {
                self.trading
                    .on_sign_failed(cx, &mut self.account, request, error)
            }
            Pending::History(request) => self.history.on_rejected(cx, request, error),
            other => self.dispatch_failed(cx, other, error),
        }
        self.failures(cx);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::BookQuery(context) => {
                let _ = cx.fail_context(&context, error);
            }
            Pending::PublicTrades(read) => history::public_trades::failed(cx, read, error),
            Pending::Identity(context) => self.identity_failed(cx, context, error),
            Pending::Leverage(context, _) => self.leverage_failed(cx, &context, error),
            Pending::History(request) => {
                let error = self.account.read_error(error);
                self.history.rejected(cx, request, error);
                self.failures(cx);
            }
            Pending::Public(request) if request.is_command_candle() => {
                request.reject_command_candle(cx, error);
                self.failures(cx);
            }
            other => self.on_sign_failed(cx, other, &error.message),
        }
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        _reason: safe_cex_engine::request_context::Error,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Public(request) if request.is_command_candle() => {
                request.reject_command_candle(cx, error);
                self.failures(cx);
            }
            Pending::Public(request) if request.is_poll_candle() => {
                self.reject_poll_candle(cx, request, error);
                self.failures(cx);
            }
            other => self.on_sign_error(cx, other, error),
        }
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::PublicTrades(read) => history::public_trades::failed(cx, read, error),
            Pending::Identity(context) => self.identity_failed(cx, context, error),
            Pending::Leverage(context, _) => self.leverage_failed(cx, &context, error),
            Pending::History(request) => {
                let error = self.account.read_error(error);
                self.history.failed(cx, request, error);
                self.failures(cx);
            }
            Pending::Public(request) if request.is_command_candle() => {
                self.command_candles_failed(cx, request, error);
                self.failures(cx);
            }
            Pending::Public(request) if request.is_poll_candle() => {
                self.poll_candles_failed(cx, request, error);
                self.failures(cx);
            }
            other => self.on_request_failed(cx, other, &error.message),
        }
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if self.account.handles_timer(timer) {
            self.account.on_timer(cx, timer);
        } else {
            self.public_timer(cx, timer);
        }
        self.failures(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchOrderBook {
            command,
            ref symbol,
            limit,
        } = command
        {
            self.fetch_book_query(cx, command, symbol, limit);
            return;
        }
        if let Command::FetchAccount { command } = command {
            self.fetch_identity(cx, command);
            self.failures(cx);
            return;
        }
        if let Some(command) = self.public_command(cx, command) {
            match command {
                Command::Query { command, query } => self.history.query(
                    cx,
                    &mut self.account,
                    command,
                    query,
                    !self.options.private_enabled(),
                ),
                other => {
                    self.trading
                        .command(cx, &mut self.account, other, &self.public.price_bands);
                }
            }
        }
        self.failures(cx);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.public.books.get(symbol).map(|s| &s.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.public.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}

impl Paradex {
    fn leverage_failed(
        &self,
        cx: &mut Cx<'_, Pending>,
        context: &safe_cex_engine::request_context::Context,
        error: safe_cex_engine::VenueError,
    ) {
        let _ = cx.fail_context(context, self.account.read_error(error));
    }
    fn dispatch_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::BookQuery(context) => {
                let _ = cx.fail_context(
                    &context,
                    depth_query::failure(error, safe_cex_engine::ErrorKind::Network),
                );
            }
            Pending::PublicTrades(read) => {
                history::public_trades::failed(cx, read, safe_cex_engine::VenueError::new(error))
            }
            Pending::Identity(context) => {
                self.identity_failed(cx, context, safe_cex_engine::VenueError::new(error))
            }
            Pending::Leverage(context, _) => self.leverage_failed(
                cx,
                &context,
                safe_cex_engine::VenueError::new(error).kind(safe_cex_engine::ErrorKind::Network),
            ),
            Pending::Public(request) => self.public_failed(cx, request, error),
            Pending::Account(request) => self.account.on_failed(cx, request, error),
            Pending::Trade(request) => {
                self.trading
                    .on_failed(cx, &mut self.account, request, error)
            }
            Pending::History(request) => self.history.on_failed(cx, request, error),
        }
    }
    fn failures(&mut self, cx: &mut Cx<'_, Pending>) {
        loop {
            let failed = self.account.drain_failed();
            if failed.is_empty() {
                break;
            }
            for (pending, error) in failed {
                match pending {
                    Pending::Trade(request) => {
                        self.trading
                            .on_refused(cx, &mut self.account, request, error)
                    }
                    Pending::History(request) => self.history.rejected(cx, request, error),
                    Pending::Identity(context) => self.identity_failed(cx, context, error),
                    Pending::Leverage(context, _) => self.leverage_failed(cx, &context, error),
                    other => self.dispatch_failed(cx, other, &error.message),
                }
            }
        }
    }
}
fn success(response: Response<'_>) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Envelope<'a> {
        #[serde(borrow)]
        error: Option<&'a serde_json::value::RawValue>,
        #[serde(borrow)]
        message: Option<&'a serde_json::value::RawValue>,
    }
    let envelope = serde_json::from_str::<Envelope<'_>>(response.body).ok();
    let error = envelope
        .as_ref()
        .and_then(|e| e.error)
        .filter(|error| !matches!(error.get(), "null" | "\"\""));
    if !(200..300).contains(&response.status) || error.is_some() {
        let mut details = Vec::new();
        if let Some(error) = error {
            if let Ok(text) = serde_json::from_str::<std::borrow::Cow<'_, str>>(error.get()) {
                details.push(text.into_owned());
            } else {
                #[derive(serde::Deserialize)]
                struct Detail {
                    code: Option<String>,
                    message: Option<String>,
                }
                if let Ok(detail) = serde_json::from_str::<Detail>(error.get()) {
                    details.extend(detail.code);
                    details.extend(detail.message);
                }
            }
        }
        if let Some(message) = envelope
            .and_then(|e| e.message)
            .and_then(|m| serde_json::from_str::<std::borrow::Cow<'_, str>>(m.get()).ok())
            .filter(|m| !m.is_empty())
        {
            details.push(message.into_owned());
        }
        let context = if (200..300).contains(&response.status) {
            "Paradex request rejected".into()
        } else {
            format!("Paradex HTTP {}", response.status)
        };
        return Err(if details.is_empty() {
            context
        } else {
            let detail: String = details.join(": ").chars().take(512).collect();
            format!("{context}: {detail}")
        });
    }
    Ok(())
}

#[cfg(test)]
mod response_tests {
    use super::*;

    #[test]
    fn rejection_retains_only_structured_venue_reasons_with_status_context() {
        for (status, body, expected) in [
            (
                400,
                r#"{"error":"INVALID_PRICE","message":"outside price bands","signature":"private"}"#,
                "Paradex HTTP 400: INVALID_PRICE: outside price bands",
            ),
            (
                200,
                r#"{"error":{"code":"REJECTED","message":"not enough margin"}}"#,
                "Paradex request rejected: REJECTED: not enough margin",
            ),
            (503, "sensitive upstream dump", "Paradex HTTP 503"),
            (
                200,
                r#"{"error":"REJECTED","message":123}"#,
                "Paradex request rejected: REJECTED",
            ),
            (
                200,
                r#"{"error":"REJECTED","message":{"detail":"denied"}}"#,
                "Paradex request rejected: REJECTED",
            ),
            (
                400,
                r#"{"message":"market is inactive"}"#,
                "Paradex HTTP 400: market is inactive",
            ),
        ] {
            assert_eq!(
                success(Response {
                    status,
                    headers: "",
                    body
                })
                .unwrap_err(),
                expected
            );
        }
        for body in ["", "{}", r#"{"error":null}"#, r#"{"error":""}"#] {
            assert!(
                success(Response {
                    status: 200,
                    headers: "",
                    body
                })
                .is_ok()
            );
        }
    }

    #[test]
    fn rejection_details_are_bounded_without_dumping_other_response_fields() {
        let body = format!(
            r#"{{"error":"REJECTED","message":"{}","headers":"not for logging"}}"#,
            "reason".repeat(200)
        );
        let error = success(Response {
            status: 400,
            headers: "",
            body: &body,
        })
        .unwrap_err();
        assert_eq!(error.chars().count(), "Paradex HTTP 400: ".len() + 512);
        assert!(!error.contains("not for logging"));
    }
}
