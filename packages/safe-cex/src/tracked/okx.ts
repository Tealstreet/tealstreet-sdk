import type { TrackedLookupResult, TrackedLookupStatus, TrackedPlacementInput, TrackedPlacementOutcome } from './types';

import { classifyPlaceOrderIds, isRecord, nonEmptyString } from './placementIds';
import { isExactDecimal, trackedQuantitiesAreValid } from './decimal';

const DUPLICATE_CL_ORD_ID = '50071';
const ORDER_NOT_FOUND = '51603';
const DEFINITIVE_REJECTIONS: Record<string, true> = {
  '51048': true,
  '51006': true,
};

const okxStatus = (value: string): TrackedLookupStatus | undefined => {
  switch (value) {
    case 'live':
      return 'open';
    case 'partially_filled':
      return 'partially_filled';
    case 'filled':
      return 'filled';
    case 'canceled':
    case 'mmp_canceled':
      return 'canceled';
    default:
      return undefined;
  }
};

const classifyOkxCode = (code: string, reason?: string): TrackedPlacementOutcome => {
  if (code === DUPLICATE_CL_ORD_ID) {
    return { kind: 'ambiguous', reason: reason ?? 'okx-duplicate-clOrdId' };
  }
  if (DEFINITIVE_REJECTIONS[code] === true) {
    return { kind: 'rejected', reason: reason ?? `okx-sCode-${code}` };
  }
  return { kind: 'ambiguous', reason: `okx-sCode-${code}` };
};

const codeFromThrownMessage = (message: string): string | undefined => {
  const match = /\(code (\d+)\)/.exec(message);
  return match?.[1];
};

/**
 * Stock OKX `placeOrders` rethrows `Error(sMsg (code sCode))` when the axios
 * body still has a row code; otherwise only `sMsg` survives.
 */
export const classifyOkxPlacement = (input: TrackedPlacementInput): TrackedPlacementOutcome => {
  if (input.kind === 'resolved') {
    return classifyPlaceOrderIds(input.value);
  }
  const error = input.error;
  if (isRecord(error) && isRecord(error.response) && isRecord(error.response.data)) {
    const data = error.response.data.data;
    if (Array.isArray(data) && data.length === 1 && isRecord(data[0]) && typeof data[0].sCode === 'string') {
      return classifyOkxCode(data[0].sCode, nonEmptyString(data[0].sMsg));
    }
  }
  if (error instanceof Error) {
    const code = codeFromThrownMessage(error.message);
    if (code !== undefined) {
      return classifyOkxCode(code, error.message);
    }
  }
  return { kind: 'ambiguous', reason: 'okx-thrown-code-lost' };
};

export const interpretOkxLookup = (
  raw: unknown,
  expectedWireId: string,
  expectedInstId: string,
): TrackedLookupResult => {
  if (!isRecord(raw) || typeof raw.code !== 'string') {
    return { kind: 'indeterminate', reason: 'okx-envelope-malformed', raw };
  }
  const data = raw.data;
  if (raw.code === ORDER_NOT_FOUND) {
    if (!Array.isArray(data) || data.length !== 0) {
      return { kind: 'indeterminate', reason: 'okx-51603-carried-rows', raw };
    }
    return { kind: 'indeterminate', reason: 'okx-absence-unconfirmable-no-finite-visibility-bound', raw };
  }
  if (raw.code === DUPLICATE_CL_ORD_ID) {
    return { kind: 'indeterminate', reason: 'okx-50071-not-absence', raw };
  }
  if (raw.code !== '0') {
    return { kind: 'indeterminate', reason: 'okx-non-success-code', raw };
  }
  if (!Array.isArray(data)) {
    return { kind: 'indeterminate', reason: 'okx-success-envelope-malformed', raw };
  }
  if (data.length === 0) {
    return { kind: 'indeterminate', reason: 'okx-empty-success-not-absence', raw };
  }
  if (data.length !== 1 || !isRecord(data[0])) {
    return { kind: 'indeterminate', reason: 'okx-multiple-or-malformed-rows', raw };
  }
  const row = data[0];
  const wireClientOrderId = nonEmptyString(row.clOrdId);
  const venueOrderId = nonEmptyString(row.ordId);
  const instId = nonEmptyString(row.instId);
  const status = nonEmptyString(row.state);
  const mapped = status === undefined ? undefined : okxStatus(status);
  const totalAmount = nonEmptyString(row.sz);
  const cumulativeFilledAmount = nonEmptyString(row.accFillSz);
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
    return { kind: 'indeterminate', reason: 'okx-row-identity-or-fields-invalid', raw };
  }
  if (!trackedQuantitiesAreValid(totalAmount, cumulativeFilledAmount, mapped)) {
    return { kind: 'indeterminate', reason: 'okx-row-quantity-invalid', raw };
  }
  const averagePrice = nonEmptyString(row.avgPx);
  if (averagePrice !== undefined && !isExactDecimal(averagePrice)) {
    return { kind: 'indeterminate', reason: 'okx-row-average-price-invalid', raw };
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
