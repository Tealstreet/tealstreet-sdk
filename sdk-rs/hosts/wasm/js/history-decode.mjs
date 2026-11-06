const SCHEMAS = new Set([
  'bitgetClassicFills', 'bitgetUtaFills', 'bitgetClassicOrders', 'bitgetUtaOrders',
  'bitgetClassicIncome', 'bitgetUtaIncome', 'bitgetPublicTrades',
  'krakenFills', 'krakenOrders', 'krakenIncome',
]);
const REASONS = new Set([
  'rowShape', 'identity', 'symbol', 'quantity', 'filledQuantity', 'timestamp',
  'timestampOrder', 'side', 'orderStatus', 'orderType', 'fee', 'market', 'economics',
]);

/** Copy only closed native row classifications across host/worker boundaries. */
export function historyDecodeOf(value) {
  try {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
    const { schema, reason } = value;
    return SCHEMAS.has(schema) && REASONS.has(reason) ? { schema, reason } : undefined;
  } catch {
    return undefined;
  }
}
