//! Worker payload preparation projects the canonical Store graph without a DTO roundtrip.
use crate::js_runtime::value::{Js, Thrown, from_typed, utf16};
use safe_cex_store::PackedTickerDelta;

pub fn should_send_update_id_backed_snapshot(
    update_id: Option<f64>,
    last_sent_update_id: Option<f64>,
    force_full_store: Option<bool>,
) -> bool {
    force_full_store == Some(true) || update_id.is_none() || last_sent_update_id != update_id
}
pub fn record_sent_update_id_backed_snapshot(update_id: Option<f64>) -> Option<f64> {
    update_id
}

struct Snapshot {
    id: Js,
    symbol: Js,
    values: [Js; 11],
}
/// Private scalar baseline; payload rows retain their own construction order.
#[derive(Default)]
pub struct TickerBaseline {
    tickers: Option<Vec<Option<Snapshot>>>,
}
impl TickerBaseline {
    pub fn copy_from(&mut self, tickers: &Js) -> Result<(), Thrown> {
        let length = tickers.get("length")?.to_number()? as usize;
        if self
            .tickers
            .as_ref()
            .is_none_or(|rows| rows.len() != length)
        {
            self.tickers = Some((0..length).map(|_| None).collect());
        }
        let baseline = self.tickers.as_mut().expect("initialized baseline");
        let mut index = 0;
        while (index as f64) < tickers.get("length")?.to_number()? {
            let ticker = tickers.get(&index.to_string())?;
            if index >= baseline.len() {
                baseline.resize_with(index + 1, || None);
            }
            if baseline[index].is_none() {
                baseline[index] = Some(Snapshot {
                    id: ticker.get("id")?,
                    symbol: ticker.get("symbol")?,
                    values: std::array::from_fn(|_| Js::Undefined),
                });
            }
            let snapshot = baseline[index].as_mut().expect("initialized row");
            snapshot.id = ticker.get("id")?;
            snapshot.symbol = ticker.get("symbol")?;
            for (value, field) in snapshot
                .values
                .iter_mut()
                .zip(safe_cex_store::store::PACKED_TICKER_FIELDS)
            {
                *value = ticker.get(field)?;
            }
            index += 1;
        }
        Ok(())
    }
    pub fn pack_deltas(&mut self, tickers: &Js) -> Result<Option<Vec<PackedTickerDelta>>, Thrown> {
        let Some(baseline) = self.tickers.as_mut() else {
            return Ok(None);
        };
        let length = tickers.get("length")?.to_number()? as usize;
        if baseline.len() != length {
            return Ok(None);
        }
        let mut packed = Vec::new();
        let mut index = 0;
        while (index as f64) < tickers.get("length")?.to_number()? {
            let ticker = tickers.get(&index.to_string())?;
            let Some(Some(previous)) = baseline.get_mut(index) else {
                return Ok(None);
            };
            if !ticker.get("id")?.strict_equals(&previous.id)
                || !ticker.get("symbol")?.strict_equals(&previous.symbol)
            {
                return Ok(None);
            }
            let mut mask = 0_u32;
            let mut changed = Vec::new();
            for (field, name) in safe_cex_store::store::PACKED_TICKER_FIELDS
                .iter()
                .enumerate()
            {
                let value = ticker.get(name)?;
                if !value.strict_equals(&previous.values[field]) {
                    previous.values[field] = value.clone();
                    if let Js::Number(value) = value {
                        mask |= 1 << field;
                        changed.push(value);
                    }
                }
            }
            if mask != 0 {
                packed.push(PackedTickerDelta {
                    index: index as f64,
                    mask: mask as f64,
                    values: changed,
                });
            }
            index += 1;
        }
        Ok(Some(packed))
    }
}

pub fn sanitize_for_post_message(
    data: &Js,
    omit_orders: bool,
    omit_positions: bool,
) -> Result<Js, Thrown> {
    let result = data.spread_object()?;
    for field in ["updateIds", "loaded", "socketHealth"] {
        let value = result.get(field)?;
        if value.truthy() {
            result.set(field, value.spread_object()?)?;
        }
    }
    for (field, omit) in [
        ("orders", omit_orders),
        ("positions", omit_positions),
        ("displayedPositions", false),
    ] {
        if omit {
            result.delete(&utf16(field))?;
            continue;
        }
        let rows = result.get(field)?;
        if !rows.is_array() {
            continue;
        }
        let mut needs = false;
        let mut index = 0;
        while (index as f64) < rows.get("length")?.to_number()? {
            if rows.get(&index.to_string())?.get("info")?.truthy() {
                needs = true;
                break;
            }
            index += 1;
        }
        if needs {
            let stripped = rows.call_method(
                "map",
                vec![Js::function("stripStoreInfo", |_, args| {
                    let item = args.first().cloned().unwrap_or(Js::Undefined);
                    if !item.get("info")?.truthy() {
                        return Ok(item);
                    }
                    // Source destructuring binds info before the rest copy excludes it.
                    item.get("info")?;
                    let row = Js::object(vec![]);
                    for key in item.own_keys() {
                        if key.as_ref() != utf16("info").as_ref()
                            && item.own_enumerability(&key)? == Some(true)
                        {
                            row.define_key(&key, item.get_key(&key)?)?;
                        }
                    }
                    Ok(row)
                })],
            )?;
            result.set(field, stripped)?;
        }
    }
    Ok(result)
}

#[derive(Default)]
pub struct StorePayloadPreparer {
    last_sent_markets_version: Option<f64>,
    last_sent_tickers_version: Option<f64>,
    last_sent_orders_update_id: Option<f64>,
    last_sent_positions_update_id: Option<f64>,
    last_payload_had_active_chasers: bool,
    baseline: TickerBaseline,
}
impl StorePayloadPreparer {
    pub fn prepare(
        &mut self,
        data: &Js,
        markets_version: f64,
        tickers_version: f64,
        hosted_mode: bool,
        force_full_store: Option<bool>,
    ) -> Result<Js, Thrown> {
        let ids = data.get("updateIds")?;
        let id = |field| -> Result<Option<f64>, Thrown> {
            Ok(if ids.is_nullish() {
                None
            } else {
                match ids.get(field)? {
                    Js::Number(value) => Some(value),
                    _ => None,
                }
            })
        };
        let orders_id = id("orders")?;
        let positions_id = id("positions")?;
        let chasers = data.get("activeChasers")?;
        let has_active_chasers = !chasers.is_nullish() && chasers.get("length")?.to_number()? > 0.0;
        let can_omit_orders = !has_active_chasers && !self.last_payload_had_active_chasers;
        let should_send_orders = !can_omit_orders
            || should_send_update_id_backed_snapshot(
                orders_id,
                self.last_sent_orders_update_id,
                force_full_store,
            );
        let should_send_positions = should_send_update_id_backed_snapshot(
            positions_id,
            self.last_sent_positions_update_id,
            force_full_store,
        );
        let payload = sanitize_for_post_message(data, !should_send_orders, !should_send_positions)?;
        let pack_ticker_deltas = !hosted_mode;
        let should_send_full_tickers = !pack_ticker_deltas
            || force_full_store == Some(true)
            || self.last_sent_tickers_version != Some(tickers_version)
            || self.baseline.tickers.is_none();
        if should_send_orders {
            self.last_sent_orders_update_id = record_sent_update_id_backed_snapshot(orders_id);
        }
        if should_send_positions {
            self.last_sent_positions_update_id =
                record_sent_update_id_backed_snapshot(positions_id);
        }
        self.last_payload_had_active_chasers = has_active_chasers;
        let tickers = payload.get("tickers")?;
        if should_send_full_tickers {
            self.last_sent_tickers_version = Some(tickers_version);
            if pack_ticker_deltas {
                self.baseline.copy_from(&tickers)?;
            }
        } else {
            match self.baseline.pack_deltas(&tickers)? {
                None => {
                    self.last_sent_tickers_version = Some(tickers_version);
                    self.baseline.copy_from(&tickers)?;
                }
                Some(deltas) => {
                    payload.delete(&utf16("tickers"))?;
                    if !deltas.is_empty() {
                        payload.set("tickerDeltas", from_typed(&deltas)?)?;
                    }
                }
            }
        }
        if force_full_store != Some(true) && self.last_sent_markets_version == Some(markets_version)
        {
            payload.delete(&utf16("markets"))?;
            return Ok(payload);
        }
        self.last_sent_markets_version = Some(markets_version);
        Ok(payload)
    }
}
