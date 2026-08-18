import type { TrackedLookupResult, TrackedLookupStatus, TrackedPlacementInput, TrackedPlacementOutcome } from './types';

import { classifyPlaceOrderIds, isRecord, nonEmptyString } from './placementIds';
import { isExactDecimal, trackedQuantitiesAreValid } from './decimal';

const DUPLICATE_ORDER_LINK_ID = 110072;
const DEFINITIVE_REJECTIONS: Record<number, true> = {
  110003: true,
  110004: true,
};

const bybitStatus = (value: string): TrackedLookupStatus | undefined => {
  switch (value) {
    case 'Created':
    case 'New':
    case 'Active':
    case 'Untriggered':
    case 'Triggered':
    case 'PendingCancel':
      return 'open';
    case 'PartiallyFilled':
      return 'partially_filled';
    case 'Filled':
      return 'filled';
    case 'Cancelled':
    case 'Deactivated':
    case 'PartiallyFilledCanceled':
      return 'canceled';
    case 'Rejected':
      return 'rejected';
    default:
      return undefined;
  }
};

const classifyBybitEnvelope = (envelope: Record<string, unknown>): TrackedPlacementOutcome => {
  const retCode = envelope.retCode;
  if (retCode === 0) {
    const result = envelope.result;
    const orderId = isRecord(result) ? nonEmptyString(result.orderId) : undefined;
    if (orderId === undefined) {
      return { kind: 'ambiguous', reason: 'bybit-ack-orderId-missing' };
    }
    return { kind: 'accepted', venueOrderId: orderId };
  }
  if (retCode === DUPLICATE_ORDER_LINK_ID) {
    return {
      kind: 'ambiguous',
      reason: nonEmptyString(envelope.retMsg) ?? 'bybit-duplicate-orderLinkId',
    };
  }
  if (typeof retCode === 'number' && DEFINITIVE_REJECTIONS[retCode] === true) {
    const retMsg = nonEmptyString(envelope.retMsg);
    return { kind: 'rejected', reason: retMsg ?? `bybit-retCode-${retCode}` };
  }
  return {
    kind: 'ambiguous',
    reason: typeof retCode === 'number' || typeof retCode === 'string' ? `bybit-retCode-${retCode}` : 'bybit-envelope-malformed',
  };
};

/**
 * Stock Bybit `placeOrder` swallows axios errors and rethrows `Error(retMsg)`,
 * dropping `retCode`. Only a resolved single ID is accepted; thrown paths stay
 * ambiguous unless an unexpected raw envelope is still attached.
 */
export const classifyBybitPlacement = (input: TrackedPlacementInput): TrackedPlacementOutcome => {
  if (input.kind === 'resolved') {
    return classifyPlaceOrderIds(input.value);
  }
  const error = input.error;
  if (isRecord(error) && isRecord(error.response) && isRecord(error.response.data)) {
    return classifyBybitEnvelope(error.response.data);
  }
  if (isRecord(error) && typeof error.retCode === 'number') {
    return classifyBybitEnvelope(error);
  }
  return { kind: 'ambiguous', reason: 'bybit-thrown-code-lost' };
};

export const interpretBybitLookup = (
  raw: unknown,
  expectedWireId: string,
  expectedSymbol: string,
): TrackedLookupResult => {
  if (!isRecord(raw)) {
    return { kind: 'indeterminate', reason: 'bybit-envelope-malformed', raw };
  }
  if (raw.retCode === DUPLICATE_ORDER_LINK_ID) {
    return {
      kind: 'indeterminate',
      reason: 'bybit-retCode-110072-not-absence',
      raw,
    };
  }
  if (raw.retCode !== 0) {
    return { kind: 'indeterminate', reason: 'bybit-non-success-retCode', raw };
  }
  if (!isRecord(raw.result) || raw.result.category !== 'linear' || !Array.isArray(raw.result.list)) {
    return { kind: 'indeterminate', reason: 'bybit-success-envelope-malformed', raw };
  }
  const list = raw.result.list;
  if (list.length === 0) {
    return { kind: 'indeterminate', reason: 'bybit-empty-list-not-absence', raw };
  }
  if (list.length !== 1) {
    return { kind: 'indeterminate', reason: 'bybit-multiple-rows', raw };
  }
  const row = list[0];
  if (!isRecord(row)) {
    return { kind: 'indeterminate', reason: 'bybit-row-malformed', raw };
  }
  const wireClientOrderId = nonEmptyString(row.orderLinkId);
  const venueOrderId = nonEmptyString(row.orderId);
  const symbol = nonEmptyString(row.symbol);
  const status = nonEmptyString(row.orderStatus);
  const mapped = status === undefined ? undefined : bybitStatus(status);
  const totalAmount = nonEmptyString(row.qty);
  const cumulativeFilledAmount = nonEmptyString(row.cumExecQty);
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
    return { kind: 'indeterminate', reason: 'bybit-row-identity-or-fields-invalid', raw };
  }
  if (!trackedQuantitiesAreValid(totalAmount, cumulativeFilledAmount, mapped)) {
    return { kind: 'indeterminate', reason: 'bybit-row-quantity-invalid', raw };
  }
  const averagePrice = nonEmptyString(row.avgPrice);
  if (averagePrice !== undefined && !isExactDecimal(averagePrice)) {
    return { kind: 'indeterminate', reason: 'bybit-row-average-price-invalid', raw };
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
