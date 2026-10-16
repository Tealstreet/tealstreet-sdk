use crate::{
    class_construction::{ConstructionOps, exchange_constructor},
    public_canonical::CanonicalOps,
};
use std::rc::Rc;
pub type Initialize<V, E> = Rc<dyn Fn(V) -> Result<(), E>>;
pub type Constructor<V, E> = Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>;
pub struct ExchangeClassOps<V, E> {
    pub construction: Rc<ConstructionOps<V, E>>,
    pub derive: Rc<dyn Fn(&str, V, usize, Constructor<V, E>, Initialize<V, E>) -> Result<V, E>>,
    pub define: Rc<dyn Fn(&V, &'static str, V) -> Result<(), E>>,
    pub unbound_member: Rc<dyn Fn(&V, &'static str, usize, bool) -> Result<V, E>>,
    pub unbound_prototype: Rc<dyn Fn(&V, &'static str, usize, bool) -> Result<(), E>>,
    pub regexp: Rc<dyn Fn(&str, &str) -> Result<V, E>>,
    pub multiply: Rc<dyn Fn(V, V) -> Result<V, E>>,
    pub null: Rc<dyn Fn() -> V>,
    pub is_string: Rc<dyn Fn(&V) -> bool>,
}
fn method<V, E>(
    h: &CanonicalOps<V, E>,
    r: V,
    key: &str,
    args: impl FnOnce() -> Result<Vec<V>, E>,
) -> Result<V, E> {
    let f = (h.get)(&r, key)?;
    (h.invoke)(f, r, args()?)
}
fn client_rule<V: Clone + 'static, E: 'static>(h: Rc<ExchangeClassOps<V, E>>) -> V {
    let value = h.construction.value.value.clone();
    let ops = h.clone();
    let mint = (value.function)(
        "mint",
        0,
        Rc::new(move |_| {
            let h = &ops.construction.value.value;
            let string = (h.global)("String")?;
            let math = (h.global)("Math")?;
            let floor = (h.get)(&math, "floor")?;
            let random = method(h, (h.global)("Math")?, "random", || Ok(vec![]))?;
            let product = (ops.multiply)(random, (h.number)(900_000_000_000_000.))?;
            let rounded = (h.invoke)(floor, math, vec![product])?;
            let number = (ops.construction.value.add)((h.number)(100_000_000_000_000.), rounded)?;
            (h.invoke)(string, (h.undefined)(), vec![number])
        }),
    );
    let ops = h.clone();
    let valid = (value.function)(
        "isValid",
        1,
        Rc::new(move |args| {
            let h = &ops.construction.value.value;
            let input = args.first().cloned().unwrap_or_else(|| (h.undefined)());
            let matches = method(h, (ops.regexp)("^[1-9]\\d*$", "")?, "test", || {
                Ok(vec![input.clone()])
            })?;
            if !(h.truthy)(&matches) {
                return Ok(matches);
            }
            method(h, (h.global)("Number")?, "isSafeInteger", || {
                let number = (h.global)("Number")?;
                Ok(vec![(h.invoke)(number, (h.undefined)(), vec![input])?])
            })
        }),
    );
    (value.object)(vec![
        ("label", (value.string)("WOO")),
        ("mint", mint),
        (
            "requirement",
            (value.string)(
                "be a canonical positive decimal integer within the JavaScript safe-integer range",
            ),
        ),
        ("isValid", valid),
    ])
}
fn arrow<V: Clone + 'static, E: 'static>(
    h: &Rc<ExchangeClassOps<V, E>>,
    receiver: &V,
    name: &'static str,
    arity: usize,
    asynchronous: bool,
) -> Result<(), E> {
    let f = if matches!(
        name,
        "normalizeTransactionSymbol" | "normalizeRequestedSymbol"
    ) {
        let ops = h.clone();
        let owner = receiver.clone();
        (h.construction.value.value.function)(
            name,
            arity,
            Rc::new(move |args| {
                let raw = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| (ops.construction.value.value.undefined)());
                if name == "normalizeTransactionSymbol" {
                    crate::response_class::normalize_transaction_symbol(&ops, raw)
                } else {
                    crate::response_class::normalize_requested_symbol(&ops, owner.clone(), raw)
                }
            }),
        )
    } else {
        (h.unbound_member)(receiver, name, arity, asynchronous)?
    };
    (h.define)(receiver, name, f)
}
fn initialize<V: Clone + 'static, E: 'static>(
    h: Rc<ExchangeClassOps<V, E>>,
    receiver: V,
) -> Result<(), E> {
    let value = &h.construction.value.value;
    (h.define)(&receiver, "clientOrderIdRule", client_rule(h.clone()))?;
    arrow(&h, &receiver, "fetchHistoricalPositions", 1, true)?;
    for key in [
        "xhr",
        "_primaryXhr",
        "_secondaryXhr",
        "_proxyEnforcedXhr",
        "publicWebsocket",
        "privateWebsocket",
    ] {
        (h.define)(&receiver, key, (value.undefined)())?;
    }
    for (key, name) in [
        ("grafanaEndpointsEvery", "GRAFANA_ENDPOINTS_EVERY"),
        ("grafanaEndpointsPeriodic", "GRAFANA_ENDPOINTS_PERIODIC"),
    ] {
        (h.define)(&receiver, key, (h.construction.value.import)(name)?)?;
    }
    (h.define)(&receiver, "_accountPositionMode", (h.null)())?;
    for (key, kind) in [
        ("_fillPositionRefreshTimers", "Map"),
        ("_fillPositionRefreshInFlight", "Set"),
        ("_fillExpectedPositionSignatureBySymbol", "Map"),
    ] {
        let constructor = (value.global)(kind)?;
        let collection = (h.construction.construct)(constructor, vec![])?;
        (h.define)(&receiver, key, collection)?;
    }
    (h.define)(&receiver, "_fundingRefreshTimer", (value.undefined)())?;
    (h.define)(
        &receiver,
        "_fundingRefreshInFlight",
        (h.construction.value.boolean)(false),
    )?;
    for key in [
        "_positionMetadataCache",
        "_positionMetadataInFlight",
        "_positionMetadataRateLimitedUntil",
    ] {
        let constructor = (value.global)("Map")?;
        let map = (h.construction.construct)(constructor, vec![])?;
        (h.define)(&receiver, key, map)?;
    }
    for (name, arity, asynchronous) in [
        ("dispose", 0, false),
        ("startFundingRatesRefresh", 0, false),
        ("parseFiniteFundingNumber", 1, false),
        ("getWooResponseMessage", 2, false),
        ("fetchAccountInfo", 0, true),
        ("refreshFundingRates", 0, true),
        ("afterMethodCalledInWorker", 2, false),
        ("normalizeTimestampSeconds", 1, false),
        ("normalizeTimestampMilliseconds", 1, false),
        ("normalizeTransactionSymbol", 1, false),
        ("normalizeRequestedSymbol", 1, false),
        ("getTransactionHistorySymbolCandidates", 1, false),
        ("parseTransactionHistoryRow", 1, false),
        ("parseTransactionRowsFromResponse", 1, false),
        ("fetchLegacyTransactionHistoryRows", 1, true),
        ("fetchTransactionHistoryRows", 1, true),
        ("getPositionSignatureForSymbol", 1, false),
        ("scheduleFillPositionRefresh", 2, false),
        ("refreshPositionForSymbolFromFill", 2, true),
        ("applyOptimisticOneWayPositionFromFill", 1, false),
        ("handleFillForPositionSync", 1, false),
        ("getAccount", 0, true),
        ("validateAccount", 0, true),
        ("start", 0, true),
        ("tick", 0, true),
        ("fetchBalance", 0, true),
        ("fetchMarkets", 0, true),
        ("fetchTickers", 0, true),
        ("fetchTickersRaw", 0, true),
        ("parseTickers", 1, false),
        ("fetchPositionsAndBalance", 0, true),
        ("createPositionMetadataCacheKey", 2, false),
        ("buildPositionMetadataFromLeverage", 3, false),
        ("fetchPositionMetadataForSymbol", 2, true),
        ("updateCachedPositionMetadataLeverage", 2, false),
        ("fetchMarketIocTouchPrice", 2, true),
        ("confirmPositionModeAndRefreshPositions", 1, true),
        ("fetchPositionMetadata", 1, true),
        ("fetchPositions", 1, true),
        ("fetchOpenPositions", 0, true),
        ("fetchOrders", 0, true),
        ("fetchHistoricalFills", 0, true),
        ("fetchHistoricalTradesInternal", 1, true),
        ("fetchOHLCV", 1, true),
        ("listenOHLCV", 2, false),
        ("listenOrderBook", 2, false),
        ("updateOrder", 1, true),
        ("updateAlgoOrder", 1, true),
        ("fetchHistoricalOrders", 1, true),
        ("fetchOrdersRecursively", 1, true),
        ("fetchLimitOrders", 0, true),
        ("mapLimitOrder", 1, false),
        ("fetchAlgoOrders", 0, true),
        ("mapAlgoOrder", 1, false),
        ("cancelOrders", 1, true),
        ("cancelAlgoOrder", 1, true),
        ("getPositionSideForOrder", 1, true),
        ("placeOrder", 1, true),
        ("placeAlgoOrder", 1, true),
        ("placeStopMarket", 1, true),
        ("placeTrailingStopLossOrder", 1, true),
        ("placePositionalAlgoOrder", 1, true),
        ("cancelAllOrders", 0, true),
        ("cancelSymbolOrders", 1, true),
        ("setLeverage", 3, true),
        ("changePositionMode", 1, true),
        ("isAlgoOrder", 1, false),
    ] {
        arrow(&h, &receiver, name, arity, asynchronous)?;
    }
    Ok(())
}
pub fn define_exchange_class<V: Clone + 'static, E: 'static>(
    h: Rc<ExchangeClassOps<V, E>>,
) -> Result<V, E> {
    let base = (h.construction.value.import)("BaseExchange")?;
    let ctor = h.construction.clone();
    let init = h.clone();
    let class = (h.derive)(
        "WOOXExchange",
        base,
        3,
        Rc::new(move |active, target, args| {
            exchange_constructor(ctor.clone(), active, target, args)
        }),
        Rc::new(move |receiver| initialize(init.clone(), receiver)),
    )?;
    (h.unbound_prototype)(&class, "getPositionMode", 0, true)?;
    for (key, value) in [
        ("MAX_FILL_POSITION_REFRESH_ATTEMPTS", 4.),
        ("FILL_POSITION_REFRESH_BASE_DELAY_MS", 150.),
        ("FUNDING_REFRESH_INTERVAL_MS", 60_000.),
    ] {
        (h.define)(&class, key, (h.construction.value.value.number)(value))?;
    }
    Ok(class)
}
