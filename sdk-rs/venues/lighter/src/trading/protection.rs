use super::*;
use safe_cex_engine::Store;

#[derive(Clone)]
pub(crate) struct Original {
    pub observed: Order,
    pub native: sign::Order,
    pub client: u64,
    pub index: u64,
    pub waiting_mark: bool,
}
#[derive(Default)]
pub(crate) struct ProtectionState {
    pub originals: HashMap<String, Original>,
    pub complex: HashSet<String>,
    pub marks: HashMap<String, (f64, f64)>,
}
pub(super) struct Edit {
    pub original: Original,
    pub replacement: Order,
    pub canceled: bool,
    pub restoring: bool,
    pub refusal: Option<VenueError>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Scalar {
    Text(String),
    Integer(u64),
    Decimal(f64),
}
impl Scalar {
    fn integer(&self) -> Option<u64> {
        match self {
            Self::Text(s) => s.parse().ok(),
            Self::Integer(n) => Some(*n),
            _ => None,
        }
    }
    fn number(&self) -> Option<f64> {
        match self {
            Self::Text(s) => s.parse().ok(),
            Self::Integer(n) => Some(*n as f64),
            Self::Decimal(n) => Some(*n),
        }
        .filter(|n| n.is_finite())
    }
}
#[derive(Default, Deserialize)]
#[serde(default)]
struct NativeRow {
    #[serde(rename = "type")]
    kind: Option<Scalar>,
    status: String,
    trigger_status: Option<String>,
    is_ask: Option<bool>,
    reduce_only: Option<bool>,
    order_index: Option<Scalar>,
    market_id: Option<Scalar>,
    market_index: Option<Scalar>,
    client_order_index: Option<Scalar>,
    initial_base_amount: Option<Scalar>,
    remaining_base_amount: Option<Scalar>,
    filled_base_amount: Option<Scalar>,
    filled_quote_amount: Option<Scalar>,
    price: Option<Scalar>,
    trigger_price: Option<Scalar>,
    order_expiry: Option<Scalar>,
    parent_order_index: Option<Scalar>,
    parent_order_id: Option<Scalar>,
    to_trigger_order_id_0: Option<Scalar>,
    to_trigger_order_id_1: Option<Scalar>,
    to_cancel_order_id_0: Option<Scalar>,
    order_flags: Option<Scalar>,
    time_in_force: Option<Scalar>,
}
impl NativeRow {
    fn field(&self, key: &str) -> Option<&Scalar> {
        match key {
            "order_index" => self.order_index.as_ref(),
            "market_id" => self.market_id.as_ref(),
            "market_index" => self.market_index.as_ref(),
            "client_order_index" => self.client_order_index.as_ref(),
            "initial_base_amount" => self.initial_base_amount.as_ref(),
            "remaining_base_amount" => self.remaining_base_amount.as_ref(),
            "filled_base_amount" => self.filled_base_amount.as_ref(),
            "filled_quote_amount" => self.filled_quote_amount.as_ref(),
            "price" => self.price.as_ref(),
            "trigger_price" => self.trigger_price.as_ref(),
            "order_expiry" => self.order_expiry.as_ref(),
            "parent_order_index" => self.parent_order_index.as_ref(),
            "parent_order_id" => self.parent_order_id.as_ref(),
            "to_trigger_order_id_0" => self.to_trigger_order_id_0.as_ref(),
            "to_trigger_order_id_1" => self.to_trigger_order_id_1.as_ref(),
            "to_cancel_order_id_0" => self.to_cancel_order_id_0.as_ref(),
            "order_flags" => self.order_flags.as_ref(),
            "time_in_force" => self.time_in_force.as_ref(),
            _ => None,
        }
    }
}
fn number(row: &NativeRow, key: &str) -> Option<f64> {
    row.field(key)?.number()
}
fn integer(row: &NativeRow, key: &str) -> Option<u64> {
    row.field(key)?.integer()
}
fn kind(row: &NativeRow) -> Option<u8> {
    match row.kind.as_ref()? {
        Scalar::Integer(n) => u8::try_from(*n)
            .ok()
            .filter(|n| matches!(n, 0 | 2 | 3 | 4 | 5)),
        Scalar::Text(s) => match s.to_ascii_lowercase().as_str() {
            "0" | "limit" => Some(0),
            "2" | "stop_loss" | "stop-loss" => Some(2),
            "3" | "stop_loss_limit" | "stop-loss-limit" => Some(3),
            "4" | "take_profit" | "take-profit" => Some(4),
            "5" | "take_profit_limit" | "take-profit-limit" => Some(5),
            _ => None,
        },
        _ => None,
    }
}
fn time_in_force(row: &NativeRow) -> Option<u64> {
    let value = row.time_in_force.as_ref()?;
    match value {
        Scalar::Text(text) => match text.as_str() {
            "immediate-or-cancel" => Some(0),
            "good-till-time" => Some(1),
            "post-only" => Some(2),
            _ => value.integer(),
        },
        _ => value.integer(),
    }
}
fn canceled(status: &str) -> bool {
    matches!(
        status,
        "canceled"
            | "cancelled"
            | "canceled-post-only"
            | "canceled-reduce-only"
            | "canceled-position-not-allowed"
            | "canceled-margin-not-allowed"
            | "canceled-too-much-slippage"
            | "canceled-not-enough-liquidity"
            | "canceled-self-trade"
            | "canceled-expired"
            | "canceled-oco"
            | "canceled-child"
            | "canceled-liquidation"
            | "canceled-invalid-balance"
    )
}
#[derive(Deserialize)]
struct Envelope {
    code: Scalar,
    #[serde(default)]
    orders: Vec<NativeRow>,
}
fn standalone(row: &NativeRow) -> bool {
    [
        "parent_order_index",
        "parent_order_id",
        "order_flags",
        "to_trigger_order_id_0",
        "to_trigger_order_id_1",
        "to_cancel_order_id_0",
    ]
    .iter()
    .all(|key| integer(row, key) == Some(0))
        && matches!(
            row.trigger_status.as_deref(),
            Some("na" | "untriggered" | "pending" | "mark-price")
        )
}
impl Original {
    pub(super) fn market_slippage_pct(&self) -> Option<f64> {
        let native = &self.native;
        if !matches!(native.order_type, 2 | 4) || native.trigger_price == 0 {
            return None;
        }
        let distance = if native.is_buy {
            native.price.checked_sub(native.trigger_price)?
        } else {
            native.trigger_price.checked_sub(native.price)?
        };
        // Recover the effective band after venue precision rounding, not a
        // stop-limit's user price or an out-of-policy native bound.
        (u64::from(distance) * 2 <= u64::from(native.trigger_price))
            .then_some(f64::from(distance) * 100.0 / f64::from(native.trigger_price))
    }

    fn from_row(store: &Store, row: &NativeRow, observed: &Order) -> Option<Self> {
        let native_kind = kind(row)?;
        if observed.status != OrderStatus::Open
            || observed.filled != 0.0
            || observed.remaining != observed.amount
        {
            return None;
        }
        if row.is_ask? != (observed.side == OrderSide::Sell)
            || row.reduce_only? != observed.reduce_only
            || number(row, "filled_base_amount")? != 0.0
            || row
                .filled_quote_amount
                .as_ref()
                .is_some_and(|n| n.number() != Some(0.0))
            || number(row, "initial_base_amount")? != observed.amount
            || number(row, "remaining_base_amount")? != observed.remaining
        {
            return None;
        }
        if !standalone(row) {
            return None;
        }
        let market = store.market(&observed.symbol)?;
        if integer(row, "order_index")?.to_string() != observed.id
            || integer(row, "market_id")
                .or_else(|| integer(row, "market_index"))?
                .to_string()
                != market.id
        {
            return None;
        }
        if (native_kind == 0 && observed.kind != OrderType::Limit)
            || (matches!(native_kind, 2 | 3) && observed.kind != OrderType::StopLoss)
            || (matches!(native_kind, 4 | 5) && observed.kind != OrderType::TakeProfit)
        {
            return None;
        }
        let meta_price = market.precision.price;
        let price = number(row, "price")?;
        let trigger = number(row, "trigger_price")?;
        if price <= 0.0
            || (native_kind != 0 && trigger != observed.price)
            || (native_kind == 0 && (trigger != 0.0 || price != observed.price))
        {
            return None;
        }
        if num::adjust(price, meta_price) != price
            || num::adjust(trigger, meta_price) != trigger
            || num::adjust(observed.amount, market.precision.amount) != observed.amount
        {
            return None;
        }
        let client = integer(row, "client_order_index")?;
        if client == 0 || client > MAX_CLIENT_INDEX {
            return None;
        }
        let tif = time_in_force(row)?;
        if tif > 2 {
            return None;
        }
        let native = sign::Order {
            market_id: market.id.parse().ok()?,
            order_id: client,
            amount: scale(observed.amount, decimals(market.precision.amount)?).ok()?,
            price: u32::try_from(scale(price, decimals(meta_price)?).ok()?).ok()?,
            trigger_price: u32::try_from(scale(trigger, decimals(meta_price)?).ok()?).ok()?,
            is_buy: observed.side == OrderSide::Buy,
            order_type: native_kind,
            time_in_force: tif as u8,
            reduce_only: observed.reduce_only,
            order_expiry: Some(integer(row, "order_expiry")?),
        };
        if native.amount == 0
            || native.amount > MAX_CLIENT_INDEX
            || native.price == 0
            || native.market_id == 255
            || native.market_id > 32767
            || native.order_expiry.is_none_or(|n| n > (1u64 << 53) - 1)
        {
            return None;
        }
        Some(Self {
            observed: observed.clone(),
            native,
            client,
            index: integer(row, "order_index")?,
            waiting_mark: row.trigger_status.as_deref() == Some("mark-price"),
        })
    }
}
fn decimals(step: f64) -> Option<u32> {
    (0..=12).find(|d| (step * 10_f64.powi(*d as i32) - 1.0).abs() < 1e-8)
}
impl Lighter {
    pub(crate) fn observe_original(&mut self, store: &Store, raw: &str, observed: &Order) {
        let parsed = serde_json::from_str::<NativeRow>(raw).ok();
        let complex = parsed.as_ref().is_some_and(|row| {
            [
                "parent_order_index",
                "parent_order_id",
                "to_trigger_order_id_0",
                "to_trigger_order_id_1",
                "to_cancel_order_id_0",
                "order_flags",
            ]
            .iter()
            .any(|key| integer(row, key).is_some_and(|n| n != 0))
        });
        if complex {
            self.protection.complex.insert(observed.id.clone());
        } else if parsed.as_ref().is_some_and(standalone) {
            self.protection.complex.remove(&observed.id);
        }
        match parsed
            .as_ref()
            .and_then(|row| Original::from_row(store, row, observed))
        {
            Some(original) => {
                self.protection
                    .originals
                    .insert(observed.id.clone(), original);
            }
            None => {
                self.protection.originals.remove(&observed.id);
            }
        }
    }
    pub(crate) fn observe_mark(&mut self, symbol: &str, field: Option<f64>, now: f64) {
        if let Some(value) = field {
            if now.is_finite() && value.is_finite() && value > 0.0 {
                self.protection.marks.insert(symbol.into(), (value, now));
            } else {
                self.protection.marks.remove(symbol);
            }
        }
    }
    pub(super) fn protective_reference(
        &self,
        store: &Store,
        order: &Order,
        now: f64,
    ) -> Result<(), VenueError> {
        if !matches!(order.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            return Ok(());
        }
        let (mark, at) = self
            .protection
            .marks
            .get(&order.symbol)
            .copied()
            .filter(|(_, at)| now.is_finite() && now >= *at && now - at <= 5000.0)
            .ok_or_else(|| not_sent("Lighter stop not changed: reference stale"))?;
        let _ = at;
        if store.ticker(&order.symbol).is_none_or(|t| t.mark != mark) {
            return Err(not_sent("Lighter stop not changed: reference stale"));
        }
        let below = (order.kind == OrderType::StopLoss) == (order.side == OrderSide::Sell);
        if (below && order.price >= mark) || (!below && order.price <= mark) {
            return Err(not_sent("Lighter stop not changed: trigger crossed"));
        }
        Ok(())
    }
    // Every edit write is revalidated at its final Claim, including nonce retries.
    // Only accepted native MARK fields grant freshness; frame arrivals never do.
    pub(crate) fn protective_release_veto(
        &self,
        store: &Store,
        pending: &Pending,
        now: f64,
    ) -> Result<(), VenueError> {
        if !matches!(pending, Pending::Trade(Request::Submit(_))) {
            return Ok(());
        }
        let Some(edit) = self
            .trading
            .active
            .as_ref()
            .and_then(|j| j.protection.as_ref())
        else {
            return Ok(());
        };
        let order = if edit.restoring {
            &edit.original.observed
        } else {
            &edit.replacement
        };
        self.protective_reference(store, order, now)?;
        let old = &edit.original.observed;
        if edit
            .original
            .native
            .order_expiry
            .is_none_or(|expiry| expiry <= now as u64)
        {
            return Err(not_sent("Lighter stop not changed: original expired"));
        }
        if old.reduce_only
            && !store.positions().iter().any(|p| {
                p.symbol == old.symbol
                    && p.contracts.is_finite()
                    && p.contracts > 0.0
                    && ((p.side == PositionSide::Long) == (old.side == OrderSide::Sell))
            })
        {
            return Err(not_sent(
                "Lighter stop not changed: protected position unavailable",
            ));
        }
        if !edit.canceled
            && (store.order(&old.id) != Some(old)
                || self.protection.complex.contains(&old.id)
                || self
                    .protection
                    .originals
                    .get(&old.id)
                    .is_none_or(|current| {
                        !native_same(&current.native, &edit.original.native)
                            || current.waiting_mark != edit.original.waiting_mark
                    }))
        {
            return Err(not_sent(
                "Lighter stop not changed: source order changed or filled",
            ));
        }
        if edit.canceled && self.trading.active.as_ref().is_some_and(|j| j.amend_filled) {
            return Err(not_sent("Lighter source filled; replacement withheld"));
        }
        Ok(())
    }
}
pub(super) fn terminal_proof(body: &str, old: &Original) -> bool {
    let Ok(value) = serde_json::from_str::<Envelope>(body) else {
        return false;
    };
    if value.code.integer() != Some(200) {
        return false;
    }
    let rows = &value.orders;
    if rows.len() != 1 {
        return false;
    }
    let row = &rows[0];
    integer(row, "order_index")
        .map(|n| n.to_string())
        .as_deref()
        == Some(old.observed.id.as_str())
        && integer(row, "market_index").or_else(|| integer(row, "market_id"))
            == Some(old.native.market_id as u64)
        && integer(row, "client_order_index") == Some(old.client)
        && canceled(&row.status)
        && number(row, "filled_base_amount") == Some(0.0)
        && row
            .filled_quote_amount
            .as_ref()
            .is_none_or(|n| n.number() == Some(0.0))
        && number(row, "initial_base_amount") == Some(old.observed.amount)
}

impl Lighter {
    pub(super) fn protective_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        error: &mut VenueError,
    ) -> bool {
        let Some(edit) = self
            .trading
            .active
            .as_ref()
            .and_then(|j| j.protection.as_ref())
        else {
            return false;
        };
        if !edit.canceled {
            return false;
        }
        if !edit.restoring && error.outcome == Some(Outcome::Rejected) {
            let mut native = edit.original.native.clone();
            let mut order = edit.original.observed.clone();
            let expiry = native.order_expiry.unwrap_or(0);
            if expiry > cx.protocol_now() as u64
                && !self.trading.active.as_ref().unwrap().amend_filled
                && let Ok(id) = self.mint_id(cx)
            {
                native.order_id = id;
                order.id = id.to_string();
                order.client_order_id = None;
                order.last_update = cx.now;
                let job = self.trading.active.as_mut().unwrap();
                let edit = job.protection.as_mut().unwrap();
                edit.restoring = true;
                edit.refusal = Some(error.clone());
                job.steps = VecDeque::from([
                    Step::Place(vec![Prepared { native, order }]),
                    Step::RestoreProof,
                ]);
                job.effect = None;
                job.delay_ms = 0;
                job.retried_nonce = true;
                self.trade_kick(cx);
                return true;
            }
        }
        let old = &self
            .trading
            .active
            .as_ref()
            .unwrap()
            .protection
            .as_ref()
            .unwrap()
            .original
            .observed;
        if error.outcome == Some(Outcome::Unknown) {
            error.message = format!(
                "Your {} protection outcome unknown; check Open Orders: {}",
                old.symbol, error.message
            );
            error.protection = None;
        } else {
            error.message = format!(
                "Your {} protection must be re-placed: {}; original protection is no longer open",
                old.symbol, error.message
            );
            error.protection = Some(safe_cex_engine::error::Protection::Unprotected);
        }
        error.presents = true;
        false
    }
}
pub(super) fn restored_proof(body: &str, edit: &Edit, client: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Envelope>(body) else {
        return false;
    };
    if value.code.integer() != Some(200) {
        return false;
    }
    let rows = &value.orders;
    if rows.len() != 1 {
        return false;
    }
    let row = &rows[0];
    let native = &edit.original.native;
    integer(row, "client_order_index")
        .map(|n| n.to_string())
        .as_deref()
        == Some(client)
        && integer(row, "order_index").is_some_and(|id| id > 0)
        && integer(row, "market_index").or_else(|| integer(row, "market_id"))
            == Some(native.market_id as u64)
        && Some(row.status.as_str()) == Some("open")
        && standalone(row)
        && kind(row) == Some(native.order_type)
        && row.is_ask == Some(!native.is_buy)
        && row.reduce_only == Some(native.reduce_only)
        && number(row, "initial_base_amount") == Some(edit.original.observed.amount)
        && number(row, "remaining_base_amount") == Some(edit.original.observed.amount)
        && number(row, "filled_base_amount") == Some(0.0)
        && row
            .filled_quote_amount
            .as_ref()
            .is_none_or(|n| n.number() == Some(0.0))
        && time_in_force(row) == Some(native.time_in_force as u64)
        && integer(row, "order_expiry") == native.order_expiry
        && number(row, "trigger_price")
            == Some(if native.order_type == 0 {
                0.0
            } else {
                edit.original.observed.price
            })
        && number(row, "price").is_some_and(|p| {
            let trigger = edit.original.observed.price;
            let native_trigger = if native.trigger_price == 0 {
                native.price
            } else {
                native.trigger_price
            };
            (p * native_trigger as f64 - trigger * native.price as f64).abs() < 1e-7
        })
}

pub(crate) fn accepted_envelope(body: &str) -> bool {
    serde_json::from_str::<Envelope>(body)
        .ok()
        .is_some_and(|value| value.code.integer() == Some(200))
}

fn native_same(a: &sign::Order, b: &sign::Order) -> bool {
    (
        a.market_id,
        a.order_id,
        a.amount,
        a.price,
        a.is_buy,
        a.order_type,
        a.time_in_force,
        a.reduce_only,
        a.trigger_price,
        a.order_expiry,
    ) == (
        b.market_id,
        b.order_id,
        b.amount,
        b.price,
        b.is_buy,
        b.order_type,
        b.time_in_force,
        b.reduce_only,
        b.trigger_price,
        b.order_expiry,
    )
}
