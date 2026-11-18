const ENDPOINTS = new Set(['order', 'amend', 'cancelReplace', 'cancelAll']);
const KINDS = new Set(['limit', 'takeProfit', 'stopLoss', 'trigger', 'attached', 'trailing', 'other', 'unavailable', 'multiple']);
const IDS = new Set(['numeric', 'other', 'missing']);
const SYMBOLS = new Set(['hyphenated', 'compact', 'other', 'missing']);
const REPLACEMENTS = new Set(['observed', 'unobserved', 'notApplicable']);
const PARAMETERS = new Set([
  'cancelClientOrderId', 'cancelOrderId', 'cancelReplaceMode', 'cancelRestrictions',
  'clientOrderId', 'closePosition', 'orderId', 'positionId', 'positionSide', 'price',
  'priceProtect', 'quantity', 'reduceOnly', 'side', 'signature', 'stopGuaranteed',
  'stopPrice', 'symbol', 'timeInForce', 'timestamp', 'type', 'workingType',
]);

/** Snapshot only closed classifications; never reads extra request/error properties. */
export function bingxWriteShapeOf(value) {
  try {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
    const { endpointFamily, method, parameterNames, hasOtherParameters, idClass, idDigits, targetKind, symbolFormat, replacement } = value;
    if (!ENDPOINTS.has(endpointFamily) || !['DELETE', 'POST'].includes(method)
      || !KINDS.has(targetKind) || !IDS.has(idClass) || !SYMBOLS.has(symbolFormat)
      || !REPLACEMENTS.has(replacement) || typeof hasOtherParameters !== 'boolean'
      || !Number.isSafeInteger(idDigits) || idDigits < 0 || idDigits > 64
      || (idClass === 'numeric' ? idDigits === 0 : idDigits !== 0)
      || !Array.isArray(parameterNames)) return undefined;
    const length = parameterNames.length;
    if (length > PARAMETERS.size) return undefined;
    const names = [];
    for (let i = 0; i < length; i++) {
      const name = parameterNames[i];
      if (!PARAMETERS.has(name) || names.includes(name)) return undefined;
      names.push(name);
    }
    return { endpointFamily, method, parameterNames: names.sort(), hasOtherParameters, idClass, idDigits, targetKind, symbolFormat, replacement };
  } catch {
    return undefined;
  }
}

export function bingxWriteShapeFromError(value) {
  try {
    if (!value || typeof value !== 'object'
      || !(value.kind === 'invalidInput' || value.venueCode === '109400')) return undefined;
    return bingxWriteShapeOf(value.bingxWriteShape);
  } catch {
    return undefined;
  }
}
