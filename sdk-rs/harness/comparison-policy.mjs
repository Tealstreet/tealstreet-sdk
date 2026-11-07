// Comparisons are on by default. Every exclusion must name the exact venue,
// field and reason here; missing legacy opt-in flags never disable a field.
export const COMPARISON_EXCLUSIONS = Object.freeze({});
export const comparesField = (venue, field) => {
  const exclusions = COMPARISON_EXCLUSIONS[venue] ?? {};
  if (!Object.hasOwn(exclusions, field)) return true;
  if (typeof exclusions[field] !== 'string' || !exclusions[field].trim())
    throw new Error(`Comparison exclusion ${venue}.${field} must declare a reason`);
  return false;
};
const finiteOrNull = (value) => {
  if (typeof value !== 'number' && (typeof value !== 'string' || !value.trim())) return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
};

// Preserve both production identities until normalization. Some TS venues emit
// a native trade id in `id`, while the Rust frame's `id` is an order id.
export const captureTsFill = (fill) => ({
  id: fill.id,
  orderId: fill.orderId,
  symbol: fill.symbol,
  side: fill.side,
  amount: fill.amount,
  price: fill.price,
  executionId: fill.executionId ?? null,
  fee: fill.fee ?? null,
  timestamp: fill.timestamp ?? null,
});

// Raw native evidence: an order id must never stand in for execution identity.
export const captureRustFill = (rows, i) => ({
  market: rows.market[i],
  side: rows.side[i],
  amount: rows.rows[i * 2 + 1],
  price: rows.rows[i * 2],
  id: rows.orderId(i),
  executionId: rows.fillId?.(i),
  fee: Number.isFinite(rows.fee?.[i]) ? rows.fee[i] : undefined,
});

const tsExecutionId = (fill, venue) => {
  if (fill.id === undefined || fill.id === null || String(fill.id).trim() === '') return null;
  // These handlers emit the native transaction/trade id unconditionally.
  if (['hyperliquid', 'hyperliquidspot', 'grvt'].includes(venue)) return fill.id;
  // These handlers fall back to an order id (or a synthetic notification id)
  // when no trade id exists. Only a distinct id alongside an order id proves
  // native execution identity; unknown/ambiguous absence remains null.
  if (['woo', 'weex', 'mexc'].includes(venue)
    && fill.orderId !== undefined && fill.orderId !== null && String(fill.orderId).trim() !== ''
    && String(fill.id) !== String(fill.orderId)) return fill.id;
  return null;
};

// Missing production metadata stays null: replay never reconstructs either
// implementation's evidence from the tape or promotes a Rust order id.
export const projectFill = (fill, venue, impl = 'ts') => ({
  id: fill.orderId ?? fill.id ?? null,
  symbol: fill.symbol ?? null,
  side: fill.side ?? null,
  amount: fill.amount ?? null,
  ...(comparesField(venue, 'fillPrice') ? { price: finiteOrNull(fill.price) } : {}),
  executionId: fill.executionId ?? (impl === 'ts' ? tsExecutionId(fill, venue) : null),
  fee: finiteOrNull(fill.fee),
  timestamp: finiteOrNull(fill.timestamp),
});

// Validate the original arrays, before any fixer or keyed comparison can hide
// equal-valued duplicates by overwriting a Map entry.
export const duplicateSymbolErrors = (snapshot, label = 'snapshot') => {
  const errors = [];
  for (const field of ['markets', 'tickers']) {
    const seen = new Map();
    for (const [index, row] of (snapshot[field] ?? snapshot.store?.[field] ?? []).entries()) {
      if (seen.has(row.symbol)) errors.push(`${label}: duplicate ${field} symbol ${JSON.stringify(row.symbol)} at rows ${seen.get(row.symbol)} and ${index}`);
      else seen.set(row.symbol, index);
    }
  }
  return errors;
};

// A detached checkpoint is immutable evidence of the state at that tape
// boundary, even while the live consumer mutates typed arrays or nested rows.
export const captureCheckpoint = (point, capture) => structuredClone({ ...point, ...capture() });

// Flat rows also carry economic state. Preserve actual reported PnL even when
// the ordinary position view represents zero-size rows through settings.
export const flatPositionPnl = (positions) => positions
  .filter((position) => position.contracts === 0 && finiteOrNull(position.realizedPnl) !== null)
  .map((position) => ({ symbol: position.symbol, side: position.side,
    positionMode: position.positionMode ?? null, realizedPnl: finiteOrNull(position.realizedPnl) }))
  .sort((a, b) => a.symbol.localeCompare(b.symbol) || String(a.side).localeCompare(String(b.side)) || String(a.positionMode).localeCompare(String(b.positionMode)));

// Keep unavailable passthrough values explicit through JSON serialization.
// Presence validation detects projection-key loss; null does not prove a fact
// that neither production consumer supplied, and never substitutes for zero.
/** The fields both implementations carry, in one shape. */
export function normalizeAccount(account, venue, impl, venueName = venue.rust) {
  const order = (o) => ({
    id: o.id == null ? null : String(o.id),
    ...(comparesField(venueName, 'clientOrderId') ? { clientOrderId: o.clientOrderId ?? null } : {}),
    symbol: o.symbol ?? null,
    status: o.status ?? null,
    type: o.type ?? null,
    side: o.side ?? null,
    price: o.price ?? null,
    takeProfit: finiteOrNull(o.takeProfit),
    stopLoss: finiteOrNull(o.stopLoss),
    conditionalOrderPrice: finiteOrNull(o.extra?.conditionalOrderPrice ?? o.conditionalOrderPrice),
    parentId: o.parentId ?? null,
    amount: o.amount ?? null,
    filled: o.filled ?? null,
    ...((venue.inverseNativeAmounts || venue.coinNativeAmounts) ? { remainingNative: o.extra?.remainingNative ?? (venue.coinNativeAmounts && impl === 'ts' ? o.remaining : null) ?? null } : {}),
    ...(comparesField(venueName, 'clientOrderId') && o.type === 'limit' ? { timeInForce: o.timeInForce ?? null } : {}),
    remaining: o.remaining ?? null,
    reduceOnly: Boolean(o.reduceOnly),
    close: Boolean(o.close),
    trigger: o.tpTriggerType ?? o.slTriggerType ?? o.triggerType ?? null,
    takeProfitTrigger: o.tpTriggerType ?? (o.type === 'take_profit_market' ? o.triggerType ?? null : null),
    stopLossTrigger: o.slTriggerType ?? (['stop_market', 'trailing_stop_market'].includes(o.type) ? o.triggerType ?? null : null),
    rejectReason: o.extra?.rejectReason ?? o.rejectReason ?? venue.tsRejectReason?.(o) ?? null,
  });
  const position = (p) => ({
    symbol: p.symbol ?? null,
    side: p.side ?? null,
    entryPrice: p.entryPrice ?? null,
    contracts: p.contracts ?? null,
    notional: p.notional ?? null,
    ...((venue.inverseNativeAmounts || venue.coinNativeAmounts) ? { contractsNative: p.extra?.contractsNative ?? p.info?.contractsNative ?? null, unrealizedPnlNative: p.extra?.unrealizedPnlNative ?? (venue.coinNativeAmounts ? p.unrealizedPnl : null) ?? null } : {}),
    leverage: p.leverage ?? null,
    unrealizedPnl: p.unrealizedPnl ?? null,
    realizedPnl: finiteOrNull(p.realizedPnl),
    liquidationPrice: p.liquidationPrice ?? null,
    marginType: p.marginType ?? null,
    positionMode: p.positionMode ?? null,
    marginSize: Number.isFinite(p.marginSize) ? p.marginSize : null,
    ...(venue.compareOptionTerms ? { option: p.option?.greeks ? { greeks: p.option.greeks } : null } : {}),
  });
  const bySymbol = (a, b) => String(a.symbol ?? '').localeCompare(String(b.symbol ?? '')) || String(a.id ?? '').localeCompare(String(b.id ?? ''));
  return {
    hedged: account.hedged,
    balance: account.balance,
    orders: account.orders.map(order).sort(bySymbol),
    // Binance zero-size rows are compared through settings, as in its TS view.
    positions: (venue.rust.startsWith('binance') ? account.positions.filter((p) => p.contracts > 0) : account.positions).map(position).sort(bySymbol),
    flatPositionPnl: account.flatPositionPnl ?? flatPositionPnl(account.positions),
    settings: account.settings
      .map((s) => ({
        symbol: s.symbol ?? null,
        marginType: s.marginType ?? null,
        ...(venue.independentLeverage
          ? { buyLeverage: s.buyLeverage ?? null, sellLeverage: s.sellLeverage ?? null }
          : { leverage: s.leverage ?? null }),
      }))
      .sort(bySymbol),
    fills: account.fills.map((fill) => projectFill(fill, venueName, impl)),
  };
}

