const SCHEMAS = new Set([
  'bitgetClassicFills', 'bitgetUtaFills', 'bitgetClassicOrders', 'bitgetUtaOrders',
  'bitgetClassicIncome', 'bitgetUtaIncome', 'bitgetPublicTrades',
  'krakenFills', 'krakenOrders', 'krakenIncome',
  'bingxIncome', 'bingxOrders',
]);
const REASONS = new Set([
  'rowShape', 'identity', 'symbol', 'quantity', 'filledQuantity', 'timestamp',
  'timestampOrder', 'side', 'orderStatus', 'orderType', 'fee', 'market', 'economics',
  'quantityNegative', 'quantityZero', 'filledQuantityNegative', 'filledQuantityExceedsQuantity',
  'quantityDecimal', 'filledQuantityDecimal', 'timestampDecimal', 'lastUpdateTimestampDecimal', 'limitPriceDecimal',
  'fieldType', 'fieldValue', 'envelopeShape', 'envelopeCode', 'dataShape',
]);
const FIELDS = new Set(['root', 'code', 'msg', 'data', 'data.orders', 'row', 'row.symbol',
  'row.incomeType', 'row.income', 'row.asset', 'row.time', 'row.tranId', 'row.tradeId',
  'row.orderId', 'row.side', 'row.type', 'row.status', 'row.price', 'row.avgPrice',
  'row.origQty', 'row.executedQty', 'row.reduceOnly', 'row.positionSide', 'row.updateTime']);
const EXPECTED = new Set(['object', 'array', 'integer', 'string', 'numberOrNumericString',
  'stringOrInteger', 'booleanOrStringOrNull', 'stringOrNull', 'stringOrIntegerOrNull', 'arrayOrObject']);
const ACTUAL = new Set(['missing', 'invalidJson', 'null', 'boolean', 'number', 'string', 'array', 'object']);
const CODES = new Set(['present', 'absent', 'unavailable']);

/** Copy only closed native row classifications across host/worker boundaries. */
export function historyDecodeOf(value) {
  try {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
    const { schema, reason } = value;
    if (!SCHEMAS.has(schema) || !REASONS.has(reason)) return undefined;
    if (schema !== 'bingxIncome' && schema !== 'bingxOrders') return { schema, reason };
    const shape = value.shape;
    if (!shape || typeof shape !== 'object' || Array.isArray(shape)) return undefined;
    const { field, expected, actual, envelopeCode } = shape;
    if (!FIELDS.has(field) || !EXPECTED.has(expected) || !ACTUAL.has(actual) || !CODES.has(envelopeCode)) return undefined;
    return { schema, reason, shape: { field, expected, actual, envelopeCode } };
  } catch {
    return undefined;
  }
}
