import type { TrackedLookupResult, TrackedLookupStatus, TrackedPlacementInput, TrackedPlacementOutcome } from './types';

import { classifyPlaceOrderIds, isRecord, nonEmptyString } from './placementIds';
import { isExactDecimal, trackedQuantitiesAreValid } from './decimal';

const DUPLICATE_CLIENT_ORDER_ID = '102002';
const DEFINITIVE_REJECTIONS: Record<string, true> = {
  '152012': true,
  '152009': true,
};

const blofinStatus = (value: string): TrackedLookupStatus | undefined => {
  switch (value) {
    case 'live':
      return 'open';
    case 'partially_filled':
      return 'partially_filled';
    case 'filled':
      return 'filled';
    case 'canceled':
    case 'cancelled':
      return 'canceled';
    case 'order_failed':
      return 'rejected';
    default:
      return undefined;
  }
};

const classifyBlofinCode = (code: string, reason?: string): TrackedPlacementOutcome => {
  if (code === DUPLICATE_CLIENT_ORDER_ID) {
    return { kind: 'ambiguous', reason: reason ?? 'blofin-duplicate-clientOrderId' };
  }
  if (DEFINITIVE_REJECTIONS[code] === true) {
    return { kind: 'rejected', reason: reason ?? `blofin-code-${code}` };
  }
  return { kind: 'ambiguous', reason: `blofin-code-${code}` };
};

export const classifyBlofinPlacement = (input: TrackedPlacementInput): TrackedPlacementOutcome => {
  if (input.kind === 'resolved') {
    return classifyPlaceOrderIds(input.value);
  }
  const error = input.error;
  if (isRecord(error) && isRecord(error.response) && isRecord(error.response.data)) {
    const data = error.response.data;
    const row = Array.isArray(data.data) && data.data.length === 1 && isRecord(data.data[0]) ? data.data[0] : undefined;
    const rowCode = row?.code;
    if (typeof rowCode === 'string' || typeof rowCode === 'number') {
      return classifyBlofinCode(String(rowCode), nonEmptyString(row?.msg));
    }
    const outer = data.code;
    if (typeof outer === 'string' || typeof outer === 'number') {
      return classifyBlofinCode(String(outer), nonEmptyString(data.msg));
    }
  }
  return { kind: 'ambiguous', reason: 'blofin-thrown-code-lost' };
};

export const interpretBlofinLookup = (
  raw: unknown,
  expectedWireId: string,
  expectedInstId: string,
): TrackedLookupResult => {
  if (!isRecord(raw) || typeof raw.code !== 'string') {
    return { kind: 'indeterminate', reason: 'blofin-envelope-malformed', raw };
  }
  if (raw.code !== '0') {
    return { kind: 'indeterminate', reason: 'blofin-non-success-code', raw };
  }
  const data = raw.data;
  if (data === undefined || data === null) {
    return { kind: 'indeterminate', reason: 'blofin-absence-unconfirmable-no-finite-visibility-bound', raw };
  }
  if (Array.isArray(data) && data.length === 0) {
    return { kind: 'indeterminate', reason: 'blofin-empty-array-not-absence', raw };
  }
  if (!isRecord(data) || Array.isArray(data)) {
    return { kind: 'indeterminate', reason: 'blofin-order-detail-malformed', raw };
  }
  if (String(data.code ?? '') === DUPLICATE_CLIENT_ORDER_ID) {
    return { kind: 'indeterminate', reason: 'blofin-102002-not-absence', raw };
  }
  const wireClientOrderId = nonEmptyString(data.clientOrderId);
  const venueOrderId = nonEmptyString(data.orderId);
  const instId = nonEmptyString(data.instId);
  const status = nonEmptyString(data.state);
  const mapped = status === undefined ? undefined : blofinStatus(status);
  const totalAmount = nonEmptyString(data.size);
  const cumulativeFilledAmount = nonEmptyString(data.filledSize);
  if (
    wireClientOrderId === undefined ||
    wireClientOrderId !== expectedWireId ||
    venueOrderId === undefined ||
    instId === undefined ||
    instId !== expectedInstId ||
    mapped === undefined ||
    totalAmount === undefined ||
    cumulativeFilledAmount === undefined
  ) {
    return { kind: 'indeterminate', reason: 'blofin-row-identity-or-fields-invalid', raw };
  }
  if (!trackedQuantitiesAreValid(totalAmount, cumulativeFilledAmount, mapped)) {
    return { kind: 'indeterminate', reason: 'blofin-row-quantity-invalid', raw };
  }
  const averagePrice = nonEmptyString(data.avgFillPrice);
  if (averagePrice !== undefined && !isExactDecimal(averagePrice)) {
    return { kind: 'indeterminate', reason: 'blofin-row-average-price-invalid', raw };
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
