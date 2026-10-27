import type { WooV3AlgoOrder, WooV3FuturesPosition, WooV3RegularOrder } from './woo.v3.private';
import type { WooV3RequestInput } from './woo.v3.request';

import { WOO_V3_ENDPOINTS } from './woo.v3.contracts';

/**
 * Inert, transport-free contracts for namespaced V3 futures account configuration.
 * The live Woo exchange intentionally does not import this file.
 */

type UnknownRecord = Record<string, unknown>;

export type WooV3AccountPositionMode = 'ONE_WAY' | 'HEDGE_MODE';
export type WooV3FuturesMarginMode = 'CROSS' | 'ISOLATED';

export type WooV3AccountModeProvenance = Readonly<{
  source: 'decoded-v3-account-info';
  reconciliationId: string;
  accountMode: 'MARGIN' | 'SPOT';
  positionMode: WooV3AccountPositionMode;
}>;

export type WooV3CompletePositionModeEvidence = Readonly<{
  /** One ID across every member proves this is one atomic reconciliation evidence set. */
  account: WooV3AccountModeProvenance;
  positions: Readonly<{
    source: 'decoded-v3-futures-positions-complete-page';
    reconciliationId: string;
    complete: true;
    rows: readonly WooV3FuturesPosition[];
  }>;
  regularOrders: Readonly<{
    source: 'decoded-v3-pending-regular-orders-complete-pages';
    reconciliationId: string;
    complete: true;
    rows: readonly WooV3RegularOrder[];
  }>;
  algoOrders: Readonly<{
    source: 'decoded-v3-pending-algo-orders-complete-pages';
    reconciliationId: string;
    complete: true;
    rows: readonly WooV3AlgoOrder[];
  }>;
}>;

export type WooV3SetLeverageInput = Readonly<{
  symbol: string;
  marginMode: WooV3FuturesMarginMode;
  leverage: number;
  account: WooV3AccountModeProvenance;
}>;

export type WooV3GetLeverageInput = Readonly<{
  symbol: string;
  marginMode: WooV3FuturesMarginMode;
  positionMode: WooV3AccountPositionMode;
}>;

export type WooV3GetLeverageRequest = Readonly<{
  request: WooV3RequestInput;
  target: WooV3GetLeverageInput;
}>;

export type WooV3LeverageReadback =
  | Readonly<{
      symbol: string;
      marginMode: WooV3FuturesMarginMode;
      positionMode: 'ONE_WAY';
      details: readonly [Readonly<{ positionSide: 'BOTH'; leverage: number }>];
      timestamp: number;
    }>
  | Readonly<{
      symbol: string;
      marginMode: WooV3FuturesMarginMode;
      positionMode: 'HEDGE_MODE';
      details: readonly [
        Readonly<{ positionSide: 'LONG'; leverage: number }>,
        Readonly<{ positionSide: 'SHORT'; leverage: number }>,
      ];
      timestamp: number;
    }>;

export type WooV3SetLeverageBuildResult =
  | Readonly<{
      ready: true;
      request: WooV3RequestInput;
      target: Readonly<{
        symbol: string;
        marginMode: 'CROSS';
        positionMode: 'ONE_WAY';
        leverage: number;
      }>;
      scopeEvidence: 'official-cross-setter-ignores-positionMode-one-way-account-provenance';
    }>
  | Readonly<{
      ready: false;
      blocker:
        | 'futures-margin-account-mode-not-proven'
        | 'isolated-margin-outside-safe-cex-contract'
        | 'cross-hedge-resulting-leg-scope-requires-demo-evidence';
      target: Readonly<{
        symbol: string;
        marginMode: WooV3FuturesMarginMode;
        positionMode: WooV3AccountPositionMode;
        leverage: number;
      }>;
    }>;

export type WooV3SetPositionModeInput = Readonly<{
  targetPositionMode: WooV3AccountPositionMode;
  evidence: WooV3CompletePositionModeEvidence;
}>;

export type WooV3SetPositionModeBuildResult =
  | Readonly<{
      ready: true;
      request: WooV3RequestInput;
      currentPositionMode: WooV3AccountPositionMode;
      targetPositionMode: WooV3AccountPositionMode;
      preconditionEvidence: 'complete-reconciled-v3-positions-zero-and-pending-orders-empty';
    }>
  | Readonly<{
      ready: false;
      blocker:
        | 'account-state-evidence-incomplete-or-mixed'
        | 'futures-margin-account-mode-not-proven'
        | 'position-mode-already-active'
        | 'open-positions-must-be-closed'
        | 'pending-regular-orders-must-be-cancelled'
        | 'pending-algo-orders-must-be-cancelled';
      currentPositionMode?: WooV3AccountPositionMode;
      targetPositionMode: WooV3AccountPositionMode;
    }>;

export type WooV3AccountMutationResult = Readonly<{
  accepted: true;
  timestamp: number;
  target:
    | Readonly<{
        family: 'futures-leverage';
        symbol: string;
        marginMode: 'CROSS';
        positionMode: 'ONE_WAY';
        leverage: number;
      }>
    | Readonly<{
        family: 'position-mode';
        positionMode: WooV3AccountPositionMode;
      }>;
  stateEvidence: 'request-target-only-response-does-not-echo-resulting-state';
}>;

export class WooV3AccountConfigResponseError extends Error {
  readonly code?: number;
  readonly gatewayResponse: Readonly<UnknownRecord>;
  readonly contractStatus = 'vendor-error-envelope-undocumented' as const;

  constructor(message: string, gatewayResponse: UnknownRecord, code?: number) {
    super(message);
    this.name = 'WooV3AccountConfigResponseError';
    this.gatewayResponse = Object.freeze({ ...gatewayResponse });
    this.code = code;
  }
}

export const WOO_V3_ACCOUNT_WRITE_PARITY = Object.freeze({
  requestAndResponseFixtures: true,
  runtimeImported: false,
  credentialedDemoLifecycle: false,
  readbackRequired: true,
  leverageReadback: 'strict-one-way-both-or-hedge-long-short',
  defaultMarginMode: 'excluded-official-response-unusable',
  crossOneWayLeverage: 'descriptor-ready-request-state-only',
  crossHedgeLeverage: 'blocked-resulting-leg-scope-requires-demo-evidence',
  isolatedLeverage: 'blocked-outside-safe-cex-cross-margin-contract',
  positionModePreconditions: 'complete-reconciled-v3-positions-zero-and-pending-orders-empty',
  mutationResponseState: 'request-target-only',
} as const);

const requireRecord = (value: unknown, path: string): UnknownRecord => {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${path} must be an object`);
  return value as UnknownRecord;
};

const requireBoolean = (value: unknown, path: string): boolean => {
  if (typeof value !== 'boolean') throw new Error(`${path} must be a boolean`);
  return value;
};

const requireInteger = (value: unknown, path: string, minimum = 0): number => {
  if (!Number.isSafeInteger(value) || (value as number) < minimum) {
    throw new Error(`${path} must be a safe integer of at least ${minimum}`);
  }
  return value as number;
};

const requireString = (value: unknown, path: string): string => {
  if (typeof value !== 'string' || value.length === 0) throw new Error(`${path} must be a non-empty string`);
  return value;
};

const requireEnum = <T extends string>(value: unknown, values: readonly T[], path: string): T => {
  if (typeof value !== 'string' || !values.includes(value as T)) {
    throw new Error(`${path} must be one of ${values.join(', ')}`);
  }
  return value as T;
};

const requireExactKeys = (input: UnknownRecord, expected: readonly string[], path: string): void => {
  const actual = Object.keys(input).sort();
  const sortedExpected = [...expected].sort();
  if (actual.length !== sortedExpected.length || actual.some((key, index) => key !== sortedExpected[index])) {
    throw new Error(`${path} must contain exactly ${sortedExpected.join(', ')}`);
  }
};

const requirePerpetualSymbol = (symbol: string): string => {
  if (!/^PERP_[A-Z0-9]+_[A-Z0-9]+$/.test(symbol)) throw new Error('WOO V3 symbol must use PERP_BASE_QUOTE form');
  return symbol;
};

const validateAccountProvenance = (input: WooV3AccountModeProvenance): WooV3AccountModeProvenance => {
  const row = requireRecord(input, 'WOO V3 account-mode provenance');
  const source = requireEnum(row.source, ['decoded-v3-account-info'] as const, 'WOO V3 account-mode provenance.source');
  return Object.freeze({
    source,
    reconciliationId: requireString(row.reconciliationId, 'WOO V3 account-mode provenance.reconciliationId'),
    accountMode: requireEnum(
      row.accountMode,
      ['MARGIN', 'SPOT'] as const,
      'WOO V3 account-mode provenance.accountMode',
    ),
    positionMode: requireEnum(
      row.positionMode,
      ['ONE_WAY', 'HEDGE_MODE'] as const,
      'WOO V3 account-mode provenance.positionMode',
    ),
  });
};

export const buildWooV3GetLeverageRequest = (input: WooV3GetLeverageInput): WooV3GetLeverageRequest => {
  const target = Object.freeze({
    symbol: requirePerpetualSymbol(input.symbol),
    marginMode: requireEnum(input.marginMode, ['CROSS', 'ISOLATED'] as const, 'WOO V3 leverage read marginMode'),
    positionMode: requireEnum(
      input.positionMode,
      ['ONE_WAY', 'HEDGE_MODE'] as const,
      'WOO V3 leverage read positionMode',
    ),
  });
  return Object.freeze({
    request: Object.freeze({
      method: 'GET',
      path: WOO_V3_ENDPOINTS.FUTURES_LEVERAGE,
      query: target,
    }),
    target,
  });
};

export const buildWooV3SetLeverageRequest = (input: WooV3SetLeverageInput): WooV3SetLeverageBuildResult => {
  const symbol = requirePerpetualSymbol(input.symbol);
  const marginMode = requireEnum(
    input.marginMode,
    ['CROSS', 'ISOLATED'] as const,
    'WOO V3 futures leverage marginMode',
  );
  const leverage = requireInteger(input.leverage, 'WOO V3 futures leverage', 1);
  const account = validateAccountProvenance(input.account);
  const target = Object.freeze({ symbol, marginMode, positionMode: account.positionMode, leverage });
  if (account.accountMode !== 'MARGIN') {
    return Object.freeze({ ready: false, blocker: 'futures-margin-account-mode-not-proven', target });
  }
  if (marginMode === 'ISOLATED') {
    return Object.freeze({ ready: false, blocker: 'isolated-margin-outside-safe-cex-contract', target });
  }
  if (account.positionMode === 'HEDGE_MODE') {
    return Object.freeze({
      ready: false,
      blocker: 'cross-hedge-resulting-leg-scope-requires-demo-evidence',
      target,
    });
  }
  const readyTarget = Object.freeze({
    symbol,
    marginMode: 'CROSS' as const,
    positionMode: 'ONE_WAY' as const,
    leverage,
  });
  return Object.freeze({
    ready: true,
    request: Object.freeze({
      method: 'PUT',
      path: WOO_V3_ENDPOINTS.FUTURES_LEVERAGE,
      body: Object.freeze({ symbol, marginMode: 'CROSS', leverage }),
    }),
    target: readyTarget,
    scopeEvidence: 'official-cross-setter-ignores-positionMode-one-way-account-provenance',
  });
};

const validateSnapshotEvidence = (
  input: unknown,
  expectedSource:
    | 'decoded-v3-futures-positions-complete-page'
    | 'decoded-v3-pending-regular-orders-complete-pages'
    | 'decoded-v3-pending-algo-orders-complete-pages',
  path: string,
): Readonly<{ reconciliationId: string; rows: readonly unknown[] }> | undefined => {
  try {
    const row = requireRecord(input, path);
    requireEnum(row.source, [expectedSource] as const, `${path}.source`);
    if (row.complete !== true || !Array.isArray(row.rows)) return undefined;
    return Object.freeze({
      reconciliationId: requireString(row.reconciliationId, `${path}.reconciliationId`),
      rows: row.rows,
    });
  } catch {
    return undefined;
  }
};

const isExactDecimalZero = (value: unknown): boolean =>
  typeof value === 'string' && /^-?(?:0+(?:\.0*)?|\.0+)$/.test(value);

const positionProvesNoExposure = (input: unknown): boolean => {
  try {
    const row = requireRecord(input, 'WOO V3 position-mode position row');
    return (
      isExactDecimalZero(row.holding) &&
      isExactDecimalZero(row.pendingLongQty) &&
      isExactDecimalZero(row.pendingShortQty)
    );
  } catch {
    return false;
  }
};

export const buildWooV3SetPositionModeRequest = (input: WooV3SetPositionModeInput): WooV3SetPositionModeBuildResult => {
  const targetPositionMode = requireEnum(
    input.targetPositionMode,
    ['ONE_WAY', 'HEDGE_MODE'] as const,
    'WOO V3 target positionMode',
  );
  let account: WooV3AccountModeProvenance;
  try {
    account = validateAccountProvenance(input.evidence?.account);
  } catch {
    return Object.freeze({
      ready: false,
      blocker: 'account-state-evidence-incomplete-or-mixed',
      targetPositionMode,
    });
  }
  const positions = validateSnapshotEvidence(
    input.evidence?.positions,
    'decoded-v3-futures-positions-complete-page',
    'WOO V3 position-mode evidence.positions',
  );
  const regularOrders = validateSnapshotEvidence(
    input.evidence?.regularOrders,
    'decoded-v3-pending-regular-orders-complete-pages',
    'WOO V3 position-mode evidence.regularOrders',
  );
  const algoOrders = validateSnapshotEvidence(
    input.evidence?.algoOrders,
    'decoded-v3-pending-algo-orders-complete-pages',
    'WOO V3 position-mode evidence.algoOrders',
  );
  const evidenceRows = [positions, regularOrders, algoOrders];
  if (
    evidenceRows.some((row) => row === undefined) ||
    evidenceRows.some((row) => row!.reconciliationId !== account.reconciliationId)
  ) {
    return Object.freeze({
      ready: false,
      blocker: 'account-state-evidence-incomplete-or-mixed',
      currentPositionMode: account.positionMode,
      targetPositionMode,
    });
  }
  if (account.accountMode !== 'MARGIN') {
    return Object.freeze({
      ready: false,
      blocker: 'futures-margin-account-mode-not-proven',
      currentPositionMode: account.positionMode,
      targetPositionMode,
    });
  }
  if (account.positionMode === targetPositionMode) {
    return Object.freeze({
      ready: false,
      blocker: 'position-mode-already-active',
      currentPositionMode: account.positionMode,
      targetPositionMode,
    });
  }
  if (!positions!.rows.every(positionProvesNoExposure)) {
    return Object.freeze({
      ready: false,
      blocker: 'open-positions-must-be-closed',
      currentPositionMode: account.positionMode,
      targetPositionMode,
    });
  }
  if (regularOrders!.rows.length > 0) {
    return Object.freeze({
      ready: false,
      blocker: 'pending-regular-orders-must-be-cancelled',
      currentPositionMode: account.positionMode,
      targetPositionMode,
    });
  }
  if (algoOrders!.rows.length > 0) {
    return Object.freeze({
      ready: false,
      blocker: 'pending-algo-orders-must-be-cancelled',
      currentPositionMode: account.positionMode,
      targetPositionMode,
    });
  }
  return Object.freeze({
    ready: true,
    request: Object.freeze({
      method: 'PUT',
      path: WOO_V3_ENDPOINTS.FUTURES_POSITION_MODE,
      body: Object.freeze({ positionMode: targetPositionMode }),
    }),
    currentPositionMode: account.positionMode,
    targetPositionMode,
    preconditionEvidence: 'complete-reconciled-v3-positions-zero-and-pending-orders-empty',
  });
};

const decodeSuccess = (input: unknown, name: string): number => {
  const response = requireRecord(input, `WOO V3 ${name} response`);
  const success = requireBoolean(response.success, `WOO V3 ${name}.success`);
  if (!success) {
    const code = response.code === undefined ? undefined : requireInteger(response.code, `WOO V3 ${name}.code`);
    const message =
      response.message === undefined
        ? `WOO V3 ${name} failed with an undocumented error envelope`
        : requireString(response.message, `WOO V3 ${name}.message`);
    throw new WooV3AccountConfigResponseError(message, response, code);
  }
  requireExactKeys(response, ['success', 'timestamp'], `WOO V3 ${name} success response`);
  return requireInteger(response.timestamp, `WOO V3 ${name}.timestamp`);
};

export const decodeWooV3GetLeverageResponse = (
  input: unknown,
  build: WooV3GetLeverageRequest,
): WooV3LeverageReadback => {
  if (build.request.method !== 'GET' || build.request.path !== WOO_V3_ENDPOINTS.FUTURES_LEVERAGE) {
    throw new Error('WOO V3 futures leverage read response requires its matching request');
  }
  const response = requireRecord(input, 'WOO V3 get futures leverage response');
  const success = requireBoolean(response.success, 'WOO V3 get futures leverage.success');
  if (!success) {
    const code =
      response.code === undefined ? undefined : requireInteger(response.code, 'WOO V3 get futures leverage.code');
    const message =
      response.message === undefined
        ? 'WOO V3 get futures leverage failed with an undocumented error envelope'
        : requireString(response.message, 'WOO V3 get futures leverage.message');
    throw new WooV3AccountConfigResponseError(message, response, code);
  }
  requireExactKeys(response, ['success', 'data', 'timestamp'], 'WOO V3 get futures leverage success response');
  const timestamp = requireInteger(response.timestamp, 'WOO V3 get futures leverage.timestamp');
  const data = requireRecord(response.data, 'WOO V3 get futures leverage.data');
  requireExactKeys(data, ['symbol', 'marginMode', 'positionMode', 'details'], 'WOO V3 get futures leverage.data');
  const symbol = requirePerpetualSymbol(requireString(data.symbol, 'WOO V3 get futures leverage.data.symbol'));
  const marginMode = requireEnum(
    data.marginMode,
    ['CROSS', 'ISOLATED'] as const,
    'WOO V3 get futures leverage.data.marginMode',
  );
  const positionMode = requireEnum(
    data.positionMode,
    ['ONE_WAY', 'HEDGE_MODE'] as const,
    'WOO V3 get futures leverage.data.positionMode',
  );
  if (
    symbol !== build.target.symbol ||
    marginMode !== build.target.marginMode ||
    positionMode !== build.target.positionMode
  ) {
    throw new Error('WOO V3 get futures leverage response scope must match its request');
  }
  if (!Array.isArray(data.details)) throw new Error('WOO V3 get futures leverage.data.details must be an array');
  const details = data.details.map((inputDetail, index) => {
    const detail = requireRecord(inputDetail, `WOO V3 get futures leverage.data.details[${index}]`);
    requireExactKeys(detail, ['positionSide', 'leverage'], `WOO V3 get futures leverage.data.details[${index}]`);
    return Object.freeze({
      positionSide: requireEnum(
        detail.positionSide,
        ['BOTH', 'LONG', 'SHORT'] as const,
        `WOO V3 get futures leverage.data.details[${index}].positionSide`,
      ),
      leverage: requireInteger(detail.leverage, `WOO V3 get futures leverage.data.details[${index}].leverage`, 1),
    });
  });
  if (positionMode === 'ONE_WAY') {
    if (details.length !== 1 || details[0]?.positionSide !== 'BOTH') {
      throw new Error('WOO V3 one-way leverage readback must contain exactly one unique BOTH detail');
    }
    return Object.freeze({
      symbol,
      marginMode,
      positionMode,
      details: Object.freeze([
        Object.freeze({ positionSide: 'BOTH' as const, leverage: details[0].leverage }),
      ] as const),
      timestamp,
    });
  }
  if (
    details.length !== 2 ||
    details.filter((detail) => detail.positionSide === 'LONG').length !== 1 ||
    details.filter((detail) => detail.positionSide === 'SHORT').length !== 1
  ) {
    throw new Error('WOO V3 hedge leverage readback must contain exactly one unique LONG and SHORT detail');
  }
  const long = details.find((detail) => detail.positionSide === 'LONG')!;
  const short = details.find((detail) => detail.positionSide === 'SHORT')!;
  return Object.freeze({
    symbol,
    marginMode,
    positionMode,
    details: Object.freeze([
      Object.freeze({ positionSide: 'LONG' as const, leverage: long.leverage }),
      Object.freeze({ positionSide: 'SHORT' as const, leverage: short.leverage }),
    ] as const),
    timestamp,
  });
};

export const decodeWooV3SetLeverageResponse = (
  input: unknown,
  build: Extract<WooV3SetLeverageBuildResult, { ready: true }>,
): WooV3AccountMutationResult => {
  if (build.request.method !== 'PUT' || build.request.path !== WOO_V3_ENDPOINTS.FUTURES_LEVERAGE) {
    throw new Error('WOO V3 futures leverage response requires its matching request');
  }
  return Object.freeze({
    accepted: true,
    timestamp: decodeSuccess(input, 'set futures leverage'),
    target: Object.freeze({ family: 'futures-leverage', ...build.target }),
    stateEvidence: 'request-target-only-response-does-not-echo-resulting-state',
  });
};

export const decodeWooV3SetPositionModeResponse = (
  input: unknown,
  build: Extract<WooV3SetPositionModeBuildResult, { ready: true }>,
): WooV3AccountMutationResult => {
  if (build.request.method !== 'PUT' || build.request.path !== WOO_V3_ENDPOINTS.FUTURES_POSITION_MODE) {
    throw new Error('WOO V3 position-mode response requires its matching request');
  }
  return Object.freeze({
    accepted: true,
    timestamp: decodeSuccess(input, 'set position mode'),
    target: Object.freeze({ family: 'position-mode', positionMode: build.targetPositionMode }),
    stateEvidence: 'request-target-only-response-does-not-echo-resulting-state',
  });
};
