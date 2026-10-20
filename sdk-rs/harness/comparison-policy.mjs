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
const finiteOrNull = (value) => Number.isFinite(value) ? value : null;

// OrderFillEvent.id is an ORDER id, not an execution id. Missing production
// metadata stays null: replay must never reconstruct candidate fields from tape.
export const projectFill = (fill, venue) => ({
  id: fill.orderId ?? fill.id ?? null,
  symbol: fill.symbol,
  side: fill.side,
  amount: fill.amount,
  ...(comparesField(venue, 'fillPrice') ? { price: finiteOrNull(fill.price) } : {}),
  executionId: fill.executionId ?? null,
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

/** The fields both implementations carry, in one shape. */
export function normalizeAccount(account, venue, impl, venueName = venue.rust) {
  const order = (o) => ({
    id: String(o.id),
    ...(comparesField(venueName, 'clientOrderId') ? { clientOrderId: o.clientOrderId ?? null } : {}),
    symbol: o.symbol,
    status: o.status,
    type: o.type,
    side: o.side,
    price: o.price,
    takeProfit: finiteOrNull(o.takeProfit),
    stopLoss: finiteOrNull(o.stopLoss),
    conditionalOrderPrice: finiteOrNull(o.extra?.conditionalOrderPrice ?? o.conditionalOrderPrice),
    parentId: o.parentId ?? null,
    amount: o.amount,
    filled: o.filled,
    ...((venue.inverseNativeAmounts || venue.coinNativeAmounts) ? { remainingNative: o.extra?.remainingNative ?? (venue.coinNativeAmounts && impl === 'ts' ? o.remaining : undefined) } : {}),
    ...(comparesField(venueName, 'clientOrderId') && o.type === 'limit' ? { timeInForce: o.timeInForce ?? null } : {}),
    remaining: o.remaining,
    reduceOnly: Boolean(o.reduceOnly),
    close: Boolean(o.close),
    trigger: o.tpTriggerType ?? o.slTriggerType ?? o.triggerType ?? null,
    rejectReason: o.extra?.rejectReason ?? o.rejectReason ?? venue.tsRejectReason?.(o) ?? null,
  });
  const position = (p) => ({
    symbol: p.symbol,
    side: p.side,
    entryPrice: p.entryPrice,
    contracts: p.contracts,
    notional: p.notional,
    ...((venue.inverseNativeAmounts || venue.coinNativeAmounts) ? { contractsNative: p.extra?.contractsNative ?? p.info?.contractsNative, unrealizedPnlNative: p.extra?.unrealizedPnlNative ?? (venue.coinNativeAmounts ? p.unrealizedPnl : undefined) } : {}),
    leverage: p.leverage,
    unrealizedPnl: p.unrealizedPnl,
    realizedPnl: finiteOrNull(p.realizedPnl),
    liquidationPrice: p.liquidationPrice,
    marginType: p.marginType ?? null,
    positionMode: p.positionMode ?? null,
    marginSize: Number.isFinite(p.marginSize) ? p.marginSize : null,
    ...(venue.compareOptionTerms ? { option: p.option?.greeks ? { greeks: p.option.greeks } : null } : {}),
  });
  const bySymbol = (a, b) => a.symbol.localeCompare(b.symbol) || String(a.id ?? '').localeCompare(String(b.id ?? ''));
  return {
    hedged: account.hedged,
    balance: account.balance,
    orders: account.orders.map(order).sort(bySymbol),
    // Binance zero-size rows are compared through settings, as in its TS view.
    positions: (venue.rust.startsWith('binance') ? account.positions.filter((p) => p.contracts > 0) : account.positions).map(position).sort(bySymbol),
    settings: account.settings
      .map((s) => ({
        symbol: s.symbol,
        marginType: s.marginType,
        ...(venue.independentLeverage
          ? { buyLeverage: s.buyLeverage, sellLeverage: s.sellLeverage }
          : { leverage: s.leverage }),
      }))
      .sort(bySymbol),
    fills: account.fills.map((fill) => projectFill(fill, venueName)),
  };
}

