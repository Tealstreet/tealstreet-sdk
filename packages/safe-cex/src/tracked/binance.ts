import type { TrackedLookupResult, TrackedLookupStatus, TrackedPlacementInput, TrackedPlacementOutcome } from './types';

import { classifyPlaceOrderIds, isRecord, nonEmptyString } from './placementIds';
import { isExactDecimal, trackedQuantitiesAreValid } from './decimal';

const ORDER_DOES_NOT_EXIST = -2013;
const DUPLICATE_CLIENT_ORDER_ID = -4116;
const DEFINITIVE_REJECTIONS: Record<string, true> = {
  '-2010': true,
  '-1111': true,
  '-4164': true,
  '-2022': true,
};

const binanceStatus = (value: string): TrackedLookupStatus | undefined => {
  switch (value) {
    case 'NEW':
    case 'PENDING_NEW':
      return 'open';
    case 'PARTIALLY_FILLED':
      return 'partially_filled';
    case 'FILLED':
      return 'filled';
    case 'CANCELED':
    case 'PENDING_CANCEL':
      return 'canceled';
    case 'REJECTED':
      return 'rejected';
    case 'EXPIRED':
    case 'EXPIRED_IN_MATCH':
      return 'expired';
    default:
      return undefined;
  }
};

const classifyBinanceCode = (code: number, reason?: string): TrackedPlacementOutcome => {
  if (code === DUPLICATE_CLIENT_ORDER_ID) {
    return { kind: 'ambiguous', reason: reason ?? 'binance-duplicate-clientOrderId' };
  }
  if (DEFINITIVE_REJECTIONS[String(code)] === true) {
    return { kind: 'rejected', reason: reason ?? `binance-code-${code}` };
  }
  return { kind: 'ambiguous', reason: `binance-code-${code}` };
};

/** Stock Binance `placeRegularOrderBatch` rethrows the raw axios error. */
export const classifyBinancePlacement = (input: TrackedPlacementInput): TrackedPlacementOutcome => {
  if (input.kind === 'resolved') {
    return classifyPlaceOrderIds(input.value);
  }
  const error = input.error;
  if (isRecord(error) && isRecord(error.response) && isRecord(error.response.data)) {
    const code = error.response.data.code;
    if (typeof code === 'number') {
      return classifyBinanceCode(code, nonEmptyString(error.response.data.msg));
    }
  }
  if (isRecord(error) && typeof error.code === 'number') {
    return classifyBinanceCode(error.code, error instanceof Error ? error.message : undefined);
  }
  return { kind: 'ambiguous', reason: 'binance-thrown-code-lost' };
};

export const interpretBinanceLookup = (
  raw: unknown,
  expectedWireId: string,
  expectedSymbol: string,
): TrackedLookupResult => {
  if (!isRecord(raw)) {
    return { kind: 'indeterminate', reason: 'binance-envelope-malformed', raw };
  }
  if (raw.code === ORDER_DOES_NOT_EXIST) {
    return { kind: 'indeterminate', reason: 'binance-absence-unconfirmable-no-finite-visibility-bound', raw };
  }
  if (typeof raw.code === 'number') {
    return { kind: 'indeterminate', reason: `binance-code-${raw.code}`, raw };
  }
  const wireClientOrderId = nonEmptyString(raw.clientOrderId);
  const venueOrderId = raw.orderId !== undefined && raw.orderId !== null ? String(raw.orderId) : undefined;
  const symbol = nonEmptyString(raw.symbol);
  const status = nonEmptyString(raw.status);
  const mapped = status === undefined ? undefined : binanceStatus(status);
  const totalAmount = nonEmptyString(raw.origQty);
  const cumulativeFilledAmount = nonEmptyString(raw.executedQty);
  if (
    wireClientOrderId === undefined ||
    wireClientOrderId !== expectedWireId ||
    venueOrderId === undefined ||
    venueOrderId.length === 0 ||
    symbol === undefined ||
    symbol !== expectedSymbol ||
    mapped === undefined ||
    totalAmount === undefined ||
    cumulativeFilledAmount === undefined
  ) {
    return { kind: 'indeterminate', reason: 'binance-row-identity-or-fields-invalid', raw };
  }
  if (!trackedQuantitiesAreValid(totalAmount, cumulativeFilledAmount, mapped)) {
    return { kind: 'indeterminate', reason: 'binance-row-quantity-invalid', raw };
  }
  const averagePrice = nonEmptyString(raw.avgPrice);
  if (averagePrice !== undefined && !isExactDecimal(averagePrice)) {
    return { kind: 'indeterminate', reason: 'binance-row-average-price-invalid', raw };
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
