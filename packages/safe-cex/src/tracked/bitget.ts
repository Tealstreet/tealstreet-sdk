import type { TrackedLookupResult, TrackedLookupStatus, TrackedPlacementInput, TrackedPlacementOutcome } from './types';

import { classifyPlaceOrderIds, isRecord, nonEmptyString } from './placementIds';
import { isExactDecimal, trackedQuantitiesAreValid } from './decimal';

const DUPLICATE_CLIENT_OID = '45034';
const ORDER_DETAIL_NOT_FOUND = '40109';
const DEFINITIVE_REJECTIONS: Record<string, true> = {
  '43012': true,
  '40762': true,
  '43006': true,
  '43007': true,
  '40102': true,
  '40017': true,
  '40402': true,
};
const SYMBOL_PRODUCT_SUFFIX = '_USDT-FUTURES';

const bitgetStatus = (value: string): TrackedLookupStatus | undefined => {
  switch (value) {
    case 'live':
      return 'open';
    case 'partially_filled':
      return 'partially_filled';
    case 'filled':
      return 'filled';
    case 'canceled':
      return 'canceled';
    default:
      return undefined;
  }
};

const classifyBitgetCode = (code: string, reason?: string): TrackedPlacementOutcome => {
  if (code === DUPLICATE_CLIENT_OID) {
    return { kind: 'ambiguous', reason: reason ?? 'bitget-duplicate-clientOid' };
  }
  if (DEFINITIVE_REJECTIONS[code] === true) {
    return { kind: 'rejected', reason: reason ?? `bitget-code-${code}` };
  }
  return { kind: 'ambiguous', reason: `bitget-code-${code}` };
};

/** Stock Bitget `placeOrder` sometimes rethrows the axios error (`data.code` survives). */
export const classifyBitgetPlacement = (input: TrackedPlacementInput): TrackedPlacementOutcome => {
  if (input.kind === 'resolved') {
    return classifyPlaceOrderIds(input.value);
  }
  const error = input.error;
  if (isRecord(error) && isRecord(error.response) && isRecord(error.response.data)) {
    const code = error.response.data.code;
    if (typeof code === 'string' || typeof code === 'number') {
      return classifyBitgetCode(String(code), nonEmptyString(error.response.data.msg));
    }
  }
  return { kind: 'ambiguous', reason: 'bitget-thrown-code-lost' };
};

export const interpretBitgetLookup = (
  raw: unknown,
  expectedWireId: string,
  expectedSymbol: string,
): TrackedLookupResult => {
  if (!isRecord(raw) || typeof raw.code !== 'string') {
    return { kind: 'indeterminate', reason: 'bitget-envelope-malformed', raw };
  }
  // 40109 is the order/detail not-found envelope. Visibility delay is unproven,
  // so this stays indeterminate (not confirmed_absent).
  if (raw.code === ORDER_DETAIL_NOT_FOUND) {
    return { kind: 'indeterminate', reason: 'bitget-40109-visibility-unproven', raw };
  }
  if (raw.code !== '00000') {
    return { kind: 'indeterminate', reason: 'bitget-non-success-code', raw };
  }
  if (!isRecord(raw.data)) {
    return { kind: 'indeterminate', reason: 'bitget-order-detail-malformed', raw };
  }
  const row = raw.data;
  const rowSymbol = nonEmptyString(row.symbol);
  const symbol =
    rowSymbol !== undefined && rowSymbol.endsWith(SYMBOL_PRODUCT_SUFFIX)
      ? rowSymbol.slice(0, -SYMBOL_PRODUCT_SUFFIX.length)
      : rowSymbol;
  const wireClientOrderId = nonEmptyString(row.clientOid);
  const venueOrderId = nonEmptyString(row.orderId);
  const status = nonEmptyString(row.state);
  const mapped = status === undefined ? undefined : bitgetStatus(status);
  const totalAmount = nonEmptyString(row.size);
  const cumulativeFilledAmount = nonEmptyString(row.baseVolume);
  if (
    wireClientOrderId === undefined ||
    wireClientOrderId !== expectedWireId ||
    venueOrderId === undefined ||
    symbol === undefined ||
    symbol !== expectedSymbol ||
    mapped === undefined ||
    totalAmount === undefined ||
    cumulativeFilledAmount === undefined
  ) {
    return { kind: 'indeterminate', reason: 'bitget-row-identity-or-fields-invalid', raw };
  }
  if (!trackedQuantitiesAreValid(totalAmount, cumulativeFilledAmount, mapped)) {
    return { kind: 'indeterminate', reason: 'bitget-row-quantity-invalid', raw };
  }
  const averagePrice = nonEmptyString(row.priceAvg);
  if (averagePrice !== undefined && !isExactDecimal(averagePrice)) {
    return { kind: 'indeterminate', reason: 'bitget-row-average-price-invalid', raw };
  }
  return {
    kind: 'found',
    wireClientOrderId,
    venueOrderId,
    status: mapped,
    totalAmount,
    cumulativeFilledAmount,
    ...(averagePrice === undefined ? {} : { averagePrice }),
    raw,
  };
};
