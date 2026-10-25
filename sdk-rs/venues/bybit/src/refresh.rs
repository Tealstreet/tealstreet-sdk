//! Explicit public discovery, staged independently from startup/account reconciliation.
use super::*;
use safe_cex_engine::engine::{ExecutionHandle, Window};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy};
use serde::Deserialize;

const DEADLINE_MS: u32 = 30_000;
const MAX_PAGES: usize = 1000;
fn statuses(category: Category) -> &'static [bool] {
    if matches!(category, Category::Linear | Category::Inverse) {
        &[false, true]
    } else {
        &[false]
    }
}

#[derive(Default)]
pub(crate) struct State {
    generation: u64,
    active: Option<Walk>,
    /// A refresh supersedes late initial catalogue rows, even if this walk fails.
    pub(crate) retired_startup: bool,
    fees: HashMap<String, (Option<f64>, Option<f64>)>,
}
/// One public discovery walk; joined commands observe its engine execution.
struct Walk {
    generation: u64,
    execution: ExecutionHandle,
    lists: HashMap<(Category, bool), List>,
}
#[derive(Default)]
struct List {
    finished: bool,
    cursors: HashSet<String>,
    pages: usize,
    markets: Vec<Market>,
    classifications: HashMap<String, InstrumentClassification>,
    derivatives: HashMap<String, DerivativeProduct>,
}
pub struct Page {
    generation: u64,
    leg: Context,
    prelaunch: bool,
    markets: Vec<Market>,
    classifications: HashMap<String, InstrumentClassification>,
    derivatives: HashMap<String, DerivativeProduct>,
    rejected: bool,
}
#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "retCode")]
    code: i64,
    result: ResultPage,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultPage {
    #[serde(default)]
    category: Option<String>,
    // Required even in the streamed response, whose list is emptied by the host.
    #[serde(rename = "list")]
    _list: Vec<serde::de::IgnoredAny>,
    #[serde(default)]
    next_page_cursor: Option<String>,
}
impl BybitLinear {
    pub(crate) fn refresh_markets(&mut self, cx: &mut Cx<'_, Pending>) {
        let Ok(root) = cx.admit_http_command(refresh_policy()) else {
            return; // The engine owns initial admission rejection.
        };
        if cx.public_feed_policy() == PublicFeedPolicy::Shared {
            let refusal = VenueError::new(
                "Refresh the shared public feed source, then restart provider and accounts",
            )
            .kind(ErrorKind::Unsupported)
            .outcome(safe_cex_engine::Outcome::NotSent);
            let _ = cx.fail_context(&root, refusal);
            return;
        }
        if let Some(walk) = self.market_refresh.active.as_ref() {
            match cx.join_execution(&walk.execution, &root, "") {
                Ok(_) => return,
                // The walk's execution already ended; start a fresh one.
                Err(Error::Revoked) => self.market_refresh.active = None,
                Err(error) => {
                    let _ = cx.fail_context(&root, error.venue_error());
                    return;
                }
            }
        }
        // An optional product that degraded during startup must recover through
        // restart; refresh never activates a new account product as a side effect.
        if !self.catalog_ready
            || !self.tickers_ready
            || self
                .categories()
                .iter()
                .any(|category| !self.catalogs.contains_key(category))
        {
            let _ = cx.fail_context(
                &root,
                VenueError::new("Bybit market catalogue is not loaded")
                    .kind(ErrorKind::InvalidInput),
            );
            return;
        }
        if cx.now < self.discovery_paused_until {
            let _ = cx.fail_context(
                &root,
                VenueError::new("Bybit public discovery is rate limited")
                    .kind(ErrorKind::RateLimited)
                    .retry_after_ms(self.discovery_paused_until - cx.now),
            );
            return;
        }
        self.market_refresh.retired_startup = true;
        self.market_refresh.generation = self.market_refresh.generation.wrapping_add(1);
        let generation = self.market_refresh.generation;
        let categories = self.categories();
        let lists = categories
            .iter()
            .flat_map(|category| {
                statuses(*category)
                    .iter()
                    .map(move |prelaunch| ((*category, *prelaunch), List::default()))
            })
            .collect();
        let execution = match cx.begin_execution(&root, refresh_policy(), Window::UntilTerminal, "")
        {
            Ok(execution) => execution,
            Err(error) => {
                let _ = cx.fail_context(&root, error.venue_error());
                return;
            }
        };
        self.market_refresh.active = Some(Walk {
            generation,
            execution,
            lists,
        });
        let selected = self.category;
        for category in categories {
            self.select_category(category);
            for prelaunch in statuses(category) {
                self.request_refresh_page(cx, generation, *prelaunch, None);
            }
        }
        self.select_category(selected);
    }
    fn request_refresh_page(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        generation: u64,
        prelaunch: bool,
        cursor: Option<&str>,
    ) {
        if self
            .market_refresh
            .active
            .as_ref()
            .is_none_or(|walk| walk.generation != generation)
        {
            return;
        }
        if cx.now < self.discovery_paused_until {
            self.fail_refresh(
                cx,
                VenueError::new("Bybit public discovery is rate limited")
                    .kind(ErrorKind::RateLimited)
                    .retry_after_ms(self.discovery_paused_until - cx.now),
            );
            return;
        }
        let mut query = Query::new().str("category", self.paths.category);
        if self.category != Category::Spot {
            query = query.num("limit", 1000.0);
        }
        if self.category == Category::Option {
            query = query.str("baseCoin", "All");
        }
        if prelaunch {
            query = query.str("status", "PreLaunch");
        }
        if let Some(cursor) = cursor {
            query = query.str("cursor", cursor);
        }
        // Invalid options never send; every observer fails now, not at expiry.
        if let Some(error) = self.config_error.clone() {
            let error = VenueError::new(error)
                .kind(ErrorKind::InvalidInput)
                .outcome(safe_cex_engine::Outcome::NotSent);
            self.fail_refresh(cx, error);
            return;
        }
        let walk = self
            .market_refresh
            .active
            .as_ref()
            .expect("generation checked");
        let leg = match cx.http_leg_context(walk.execution.context(), refresh_policy()) {
            Ok(leg) => leg,
            Err(error) => {
                self.fail_refresh(cx, error.venue_error());
                return;
            }
        };
        let rest = Rest::get("/v5/market/instruments-info", query).streamed();
        let url = format!("{}{}?{}", self.paths.rest, rest.path, rest.payload);
        let pending = Pending {
            category: self.category,
            rest,
            then: Then::RefreshMarkets(Page {
                generation,
                leg: leg.clone(),
                prelaunch,
                markets: Vec::new(),
                classifications: HashMap::new(),
                derivatives: HashMap::new(),
                rejected: false,
            }),
            read: None,
        };
        match cx.request_managed_with_headers(
            leg,
            None,
            Method::Get,
            url,
            &[("X-Referer", sign::BROKER_ID)],
            None,
            pending,
        ) {
            Ok(request) => cx.stream_response(request, ArrayAt::Path(MARKETS_PATH)),
            Err((_, error)) => self.fail_refresh(cx, error.venue_error()),
        }
    }
    pub(crate) fn refresh_item(&mut self, page: &mut Page, item: &str) {
        if self
            .market_refresh
            .active
            .as_ref()
            .is_none_or(|walk| walk.generation != page.generation)
        {
            return;
        }
        let Ok(row) = decode_instrument(item) else {
            page.rejected = true;
            return;
        };
        if self.variant.is_hyro()
            && (row.quote_coin != "USDT"
                || (!row.contract_type.is_empty() && row.contract_type != "LinearPerpetual"))
        {
            return;
        }
        if self.category == Category::Option && row.status != "Trading" {
            return;
        }
        let product = derivative_from_wire(&row, self.category);
        if self.category == Category::Option
            && (row.base_coin.trim().is_empty()
                || row.quote_coin.trim().is_empty()
                || product.as_ref().is_none_or(|product| {
                    product
                        .expiry
                        .is_none_or(|expiry| !expiry.is_finite() || expiry <= 0.0)
                        || product
                            .settlement_asset
                            .as_deref()
                            .is_none_or(|asset| asset.trim().is_empty())
                }))
        {
            // A malformed Trading option is not evidence that it was delisted.
            // Refuse the walk before replacing any known option terms.
            page.rejected = true;
            return;
        }
        let market = market_from_wire(&row, self.variant, self.category);
        if market.symbol.is_empty() || !market.precision.is_tradable() {
            page.rejected = true;
            return;
        }
        page.classifications
            .insert(market.symbol.clone(), classification_from_wire(&row));
        if let Some(product) = product {
            page.derivatives.insert(market.symbol.clone(), product);
        }
        page.markets.push(market);
    }
    pub(crate) fn refresh_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        page: Page,
        response: Response<'_>,
    ) {
        // Publication gate before any native state or Store change.
        if self.market_refresh.active.as_ref().is_none_or(|walk| {
            walk.generation != page.generation
                || cx
                    .validate_execution_read_delivery(&walk.execution, &page.leg)
                    .is_err()
        }) {
            return;
        }
        let pause = discovery_rate_limit_pause_ms(&response, cx.now);
        self.discovery_paused_until = self.discovery_paused_until.max(cx.now + pause);
        if let Some((code, message)) = venue_error(&response) {
            let kind = if pause > 0.0 {
                ErrorKind::RateLimited
            } else {
                ErrorKind::VenueUnavailable
            };
            self.fail_refresh(
                cx,
                VenueError::new(message)
                    .kind(kind)
                    .venue_code(code.to_string()),
            );
            return;
        }
        let envelope = match serde_json::from_str::<Envelope>(response.body) {
            Ok(envelope)
                if envelope.code == 0
                    && !page.rejected
                    && envelope
                        .result
                        .category
                        .as_deref()
                        .is_none_or(|category| category == self.paths.category) =>
            {
                envelope
            }
            _ => {
                self.fail_refresh(
                    cx,
                    VenueError::new("Malformed Bybit market refresh response")
                        .kind(ErrorKind::Internal),
                );
                return;
            }
        };
        let walk = self
            .market_refresh
            .active
            .as_mut()
            .expect("generation checked");
        let list = walk
            .lists
            .get_mut(&(self.category, page.prelaunch))
            .expect("enabled category");
        if list.finished {
            return;
        }
        list.pages += 1;
        list.markets.extend(page.markets);
        list.classifications.extend(page.classifications);
        list.derivatives.extend(page.derivatives);
        let cursor = envelope
            .result
            .next_page_cursor
            .filter(|cursor| !cursor.is_empty());
        // Native spot instruments have no pagination, even if a gateway echoes a cursor.
        if let Some(cursor) = cursor.filter(|_| self.category != Category::Spot) {
            if list.pages >= MAX_PAGES || !list.cursors.insert(cursor.clone()) {
                self.fail_refresh(
                    cx,
                    VenueError::new("Bybit market refresh pagination did not progress")
                        .kind(ErrorKind::Internal),
                );
                return;
            }
            self.request_refresh_page(cx, page.generation, page.prelaunch, Some(&cursor));
            return;
        }
        list.finished = true;
        if walk.lists.values().all(|list| list.finished) {
            self.commit_refresh(cx);
        }
    }
    pub(crate) fn refresh_failed(&mut self, cx: &mut Cx<'_, Pending>, page: Page, message: &str) {
        if self
            .market_refresh
            .active
            .as_ref()
            .is_some_and(|walk| walk.generation == page.generation)
        {
            self.fail_refresh(cx, VenueError::new(message).kind(ErrorKind::Network));
        }
    }
    fn fail_refresh(&mut self, cx: &mut Cx<'_, Pending>, error: VenueError) {
        if let Some(walk) = self.market_refresh.active.take() {
            // An already-ended execution has no owner left to fail.
            let _ = cx.abort_execution(&walk.execution, error);
        }
    }
    fn commit_refresh(&mut self, cx: &mut Cx<'_, Pending>) {
        let mut walk = self.market_refresh.active.take().expect("completed walk");
        let mut catalogs = HashMap::new();
        let mut classifications = HashMap::new();
        let mut derivatives = HashMap::new();
        let mut markets = Vec::new();
        for category in self.categories() {
            let mut rows = Vec::<Market>::new();
            let mut indices = HashMap::new();
            for &prelaunch in statuses(category) {
                let list = walk
                    .lists
                    .get_mut(&(category, prelaunch))
                    .expect("completed category");
                for market in std::mem::take(&mut list.markets) {
                    if let Some(&index) = indices.get(&market.symbol) {
                        rows[index] = market;
                    } else {
                        indices.insert(market.symbol.clone(), rows.len());
                        rows.push(market);
                    }
                }
                classifications.extend(std::mem::take(&mut list.classifications));
                derivatives.extend(std::mem::take(&mut list.derivatives));
            }
            catalogs.insert(category, rows.clone());
            markets.extend(rows);
        }
        if markets.is_empty() {
            let _ = cx.abort_execution(
                &walk.execution,
                VenueError::new("Bybit market refresh returned an empty catalogue")
                    .kind(ErrorKind::VenueUnavailable),
            );
            return;
        }
        // Capture at commit, so concurrent account fee/settings changes win over stale discovery.
        for market in cx.store.markets() {
            let fees = self
                .market_refresh
                .fees
                .entry(market.symbol.clone())
                .or_default();
            if market.taker_fee.is_some() {
                fees.0 = market.taker_fee;
            }
            if market.maker_fee.is_some() {
                fees.1 = market.maker_fee;
            }
        }
        for market in &mut markets {
            if let Some(&(taker, maker)) = self.market_refresh.fees.get(&market.symbol) {
                market.taker_fee = taker;
                market.maker_fee = maker;
            }
        }
        let old: HashSet<_> = cx
            .store
            .markets()
            .iter()
            .map(|market| market.symbol.clone())
            .collect();
        let added: Vec<_> = markets
            .iter()
            .filter(|market| !old.contains(&market.symbol))
            .map(|market| market.symbol.clone())
            .collect();
        cx.store.set_markets(markets);
        // Store settings are already keyed by symbol, including removed markets.
        // set_markets preserves them; do not replace that financial cache.
        publish_product_metadata(cx.store, &classifications, &derivatives);
        for symbol in &added {
            cx.store.ensure_ticker(symbol);
        }
        self.catalogs = catalogs;
        self.classification = classifications;
        self.derivatives = derivatives;
        self.published_catalogs = self.catalogs.len();
        let selected = self.category;
        for category in self.categories() {
            self.select_category(category);
            let topics = added
                .iter()
                .filter(|symbol| self.owns_symbol(symbol))
                .map(|symbol| format!("tickers.{symbol}"))
                .collect::<Vec<_>>();
            self.subscribe(cx, "subscribe", &topics);
        }
        self.select_category(selected);
        let _ = cx.complete_execution(&walk.execution, |_| Ok("null".into()));
    }
}

fn refresh_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: DEADLINE_MS,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
