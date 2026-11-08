//! Coinbase Advanced Trade: public INTX discovery and native account products.
mod account;
mod account_wire;
pub mod drb;
pub mod drb_auth;
mod history;
mod local_context;
mod public;
mod rest;
pub mod sign;
mod trading;
mod wire;

use safe_cex_engine::book::Book;
use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason, SlotPolicy, Slots};
use safe_cex_engine::{
    Command, Cx, ErrorKind, Level, Response, SocketId, TimerId, Venue, VenueError,
};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const INTX_MOVED: &str =
    "Coinbase International moved to Coinbase's new derivatives API; an update is coming.";

fn unavailable(status: u16) -> bool {
    matches!(status, 404 | 408 | 410) || status >= 500
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Options {
    pub key: String,
    pub secret: String,
    pub public_data_only: bool,
    pub spot_only: bool,
    pub testnet: bool,
    /// A saved read-only key signs REST reads; writes and the user stream keep the primary key.
    pub read_credentials: Option<safe_cex_engine::ReadCredentials>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductKind {
    Intx,
    Cfm,
    Spot,
}
pub enum Pending {
    Public(public::Request),
    /// The flag records whether the read-only key signed the request.
    Account(account::Request, bool),
    Trading(trading::Request),
    History(history::Request),
}
pub struct Coinbase {
    /// Shared derivatives transport; stream modules opt in through this state.
    pub drb: drb::Transport,
    pub(crate) options: safe_cex_crypto::secrets::Sensitive<Options>,
    pub(crate) public: public::Public,
    pub(crate) signers: Slots<sign::Credentials>,
    // Keep the supplied-slot routing fact when a rejected read key is disabled.
    pub(crate) read_disabled: bool,
    rest: rest::Rest,
    pub(crate) account: account::Account,
    pub(crate) trading: trading::Trading,
    pub(crate) history: history::History,
    intx_startup_failed: bool,
}
impl Coinbase {
    pub fn with_signing(
        options: Options,
        primary: Option<(sign::Binding, safe_cex_engine::signing::Identity)>,
        secondary: Option<(sign::Binding, safe_cex_engine::signing::Identity)>,
    ) -> Result<Self, String> {
        let mut venue = Self::new(options)?;
        if !venue.options.public_data_only {
            let primary = primary
                .map(|(binding, identity)| sign::Credentials::external(binding, identity))
                .transpose()?;
            let secondary = secondary
                .map(|(binding, identity)| sign::Credentials::external(binding, identity))
                .transpose()?;
            venue.options.key = primary
                .as_ref()
                .map_or_else(String::new, |c| c.key().to_owned());
            venue.signers = Slots::new(primary, secondary, Self::slot_policy());
        }
        Ok(venue)
    }
    pub fn new(options: Options) -> Result<Self, String> {
        let mut options = safe_cex_crypto::secrets::Sensitive::new(options);
        let raw_key = Zeroizing::new(std::mem::take(&mut options.key));
        let secret = Zeroizing::new(std::mem::take(&mut options.secret));
        let credentials = if options.public_data_only || raw_key.is_empty() {
            None
        } else {
            Some(sign::Credentials::new(&raw_key, &secret)?)
        };
        let read = options.read_credentials.take();
        let read_credentials = match (&credentials, read) {
            (Some(_), Some(read)) => {
                read.validate()?;
                let key = Zeroizing::new(read.key);
                let secret = Zeroizing::new(read.secret);
                Some(sign::Credentials::new(&key, &secret)?)
            }
            _ => None,
        };
        options.key = credentials
            .as_ref()
            .map_or_else(String::new, |c| c.key().to_owned());
        Ok(Self {
            drb: drb::Transport::default(),
            options,
            public: public::Public::new(),
            signers: Slots::new(credentials, read_credentials, Self::slot_policy()),
            read_disabled: false,
            rest: rest::Rest::default(),
            account: account::Account::new(),
            trading: trading::Trading::default(),
            history: history::History::default(),
            intx_startup_failed: false,
        })
    }

    fn slot_policy() -> SlotPolicy {
        SlotPolicy {
            secondary_requires_primary: true,
        }
    }

    pub(crate) fn has_read_key(&self) -> bool {
        self.signers.secondary_supplied() && !self.read_disabled
    }

    pub(crate) fn read_role(&self) -> CredentialRole {
        if self.read_disabled {
            CredentialRole::PrimaryRead {
                reason: PrimaryReason::DegradedRead,
                scope: None,
            }
        } else {
            CredentialRole::Read { scope: None }
        }
    }
}
impl Coinbase {
    fn is_intx_startup_read(&self, pending: &Pending) -> bool {
        match pending {
            Pending::Public(public::Request::Instruments { initial: true, .. })
            | Pending::Public(public::Request::Products {
                kind: ProductKind::Intx,
                ..
            }) => true,
            Pending::Account(account::Request::Probe { portfolios, index }, _) => {
                portfolios.get(*index).is_some_and(|p| p.kind == "INTX")
            }
            Pending::Account(
                account::Request::Balance
                | account::Request::Positions { .. }
                | account::Request::Orders(_),
                _,
            ) => self.account.is_intx(),
            _ => false,
        }
    }

    /// Only positively identified INTX bootstrap reads can report the migration.
    /// Spot/CFM, credentials, malformed 2xx replies and running feeds keep their handling.
    fn fail_moved_intx_startup(&mut self, cx: &mut Cx<'_, Pending>, intx: bool) -> bool {
        let loaded = cx.store.loaded();
        let ready = loaded.markets
            && if self.signers.has_primary() {
                loaded.balance && loaded.orders && loaded.positions
            } else {
                loaded.tickers
            };
        if self.options.spot_only || ready {
            return false;
        }
        if !intx {
            return false;
        }
        if !std::mem::replace(&mut self.intx_startup_failed, true) {
            let error = VenueError::new(INTX_MOVED).kind(ErrorKind::Unsupported);
            cx.notice_error(
                Level::Error,
                safe_cex_engine::log::code::REQUEST_FAILED,
                &error,
            );
            cx.startup_failed(error);
        }
        true
    }

    /// The private portfolio discovered at startup is the account's identity.
    fn account_identity(&self) -> Result<String, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Identity<'a> {
            user_id: &'a str,
        }
        match self.account.portfolio_id.as_deref() {
            Some(id) if !self.options.public_data_only => {
                serde_json::to_string(&Identity { user_id: id }).map_err(|error| error.to_string())
            }
            _ => Err("Coinbase private portfolio ID is unavailable".into()),
        }
    }
}

impl Venue for Coinbase {
    fn http_plans() -> bool {
        false
    }
    fn guard_order_mutations(&self) -> bool {
        true
    }

    type Pending = Pending;
    fn startup_reads(pending: &Pending) -> safe_cex_engine::startup::Sections {
        use safe_cex_engine::startup::Sections as S;
        match pending {
            Pending::Public(
                public::Request::Products { .. } | public::Request::Instruments { .. },
            ) => S::PUBLIC,
            Pending::Public(public::Request::Drb { startup: true, .. }) => S::PUBLIC,
            Pending::Account(request, _) => request.startup_sections(),
            _ => S::NONE,
        }
    }

    fn repair_subscription(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        topic: &safe_cex_engine::SubscriptionTopic,
    ) -> Option<Result<(), String>> {
        Some(self.public_repair(cx, topic))
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        safe_cex_engine::Capabilities {
            fill_history_time: safe_cex_engine::FillHistoryTime::SecondsOrMilliseconds,
            finite_command_numbers: true,
            finite_history_bounds: true,
            reject_order_history_filters: true,
            quantity_units: safe_cex_engine::QuantityUnits::NativePlacement,
            income_history: false,
            account: safe_cex_engine::AccountLookup::AfterLoad,
            leverage_query: safe_cex_engine::LeverageQuery::OnRead,
            ..safe_cex_engine::Capabilities::BASE
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.signers.has_primary() {
            self.private_start(cx);
        } else {
            self.public_start(cx);
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if !self.public_open(cx, socket) {
            self.private_on_open(cx, socket);
        }
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        if !self.public_message(cx, socket, text) {
            self.private_on_message(cx, socket, text);
        }
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, _code: u16) {
        if !self.public_close(cx, socket) {
            self.private_on_close(cx, socket);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        // Instrument error arrays are decoded by public_response before classifying availability.
        let intx = self.is_intx_startup_read(&pending)
            && !matches!(
                pending,
                Pending::Public(public::Request::Instruments { .. })
            );
        if unavailable(response.status) && self.fail_moved_intx_startup(cx, intx) {
            self.on_request_failed(cx, pending, INTX_MOVED);
            return;
        }
        match pending {
            Pending::Public(p) => self.public_response(cx, p, response),
            Pending::Account(p, read) => self.private_on_response(cx, p, read, response),
            Pending::Trading(p) => self.trading_response(cx, p, response),
            Pending::History(p) => self.history_response(cx, p, response),
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        match pending {
            Pending::Public(p) => self.public_item(cx, p, item),
            Pending::Account(p, _) => self.private_on_item(cx, p, item),
            Pending::Trading(_) => {}
            Pending::History(p) => self.history_item(cx, p, item),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Public(p) => self.public_failed(cx, p, error),
            Pending::Account(p, _) => self.private_failed(cx, p, error),
            Pending::Trading(p) => self.trading_failed(cx, p, error),
            Pending::History(p) => self.history_failed(cx, p, error),
        }
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        cx.with_startup_request_failure(pending, error, |cx, pending, error| {
            self.fail_unsent(cx, pending, &error.message);
        });
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        let intx = self.is_intx_startup_read(&pending);
        if matches!(error.kind, Some(ErrorKind::Network | ErrorKind::Timeout))
            && self.fail_moved_intx_startup(cx, intx)
        {
            self.on_request_failed(cx, pending, INTX_MOVED);
            return;
        }
        self.on_request_failed(cx, pending, &error.message);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if !self.rest_timer(cx, timer) && !self.public_timer(cx, timer) {
            self.private_on_timer(cx, timer);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::FetchAccount { .. } = command {
            local_context::complete(
                cx,
                safe_cex_engine::request_context::Operation::Read,
                self.account_identity()
                    .map_err(safe_cex_engine::VenueError::new),
            );
            return;
        }
        if let Some(Command::Query { command, query }) = self
            .public_command(cx, command)
            .and_then(|command| self.trading_command(cx, command))
        {
            self.history_start(cx, command, query);
        }
    }

    fn book(&self, symbol: &str) -> Option<&Book> {
        self.public
            .books
            .get(symbol)
            .filter(|b| b.live)
            .map(|b| &b.book)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(self.public.books.iter().map(|(s, b)| (s.as_str(), &b.book)))
    }
}
