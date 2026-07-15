/**
 * Inert account-mode detection contract for a future KuCoin UTA migration.
 *
 * This module owns no transport and is not imported by the active KuCoin
 * integrations. The existing integrations are Classic-only. A future caller
 * must positively identify CLASSIC before selecting them; every other result
 * fails closed.
 */

export const KUCOIN_UTA_ACCOUNT_MODE_ROUTE = Object.freeze({
  baseUrl: 'https://api.kucoin.com',
  method: 'GET',
  path: '/api/ua/v1/account/mode',
} as const);

export const KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE = '200000' as const;
export const KUCOIN_UTA_ACCOUNT_MODES = ['CLASSIC', 'UNIFIED'] as const;

/** KuCoin prohibits production use of UTA while its API remains unreleased. */
export const KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED = false as const;

export type KucoinAccountMode = (typeof KUCOIN_UTA_ACCOUNT_MODES)[number];

export type KucoinAccountModeDetection =
  | Readonly<{
      accountMode: KucoinAccountMode;
      classicSubAccount: readonly number[];
      status: 'identified';
      unifiedSubAccount: readonly number[];
    }>
  | Readonly<{
      reason: 'unknown-self-account-mode';
      status: 'unknown';
    }>
  | Readonly<{
      reason: 'invalid-envelope' | 'invalid-success-code' | 'invalid-data' | 'invalid-self-account-mode';
      status: 'malformed';
    }>;

export type KucoinClassicRuntimeDecision =
  | Readonly<{
      eligible: true;
      reason: 'classic-account-mode-confirmed';
      route: 'existing-classic-runtime';
      utaRuntimeActivationEnabled: false;
    }>
  | Readonly<{
      eligible: false;
      reason: 'unified-account-mode-unsupported' | 'account-mode-unknown' | 'account-mode-response-malformed';
      route: 'blocked';
      utaRuntimeActivationEnabled: false;
    }>;

type PlainRecord = Record<PropertyKey, unknown>;

const isPlainRecord = (value: unknown): value is PlainRecord => {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
};

const hasExactEnumerableDataKeys = (value: PlainRecord, expectedKeys: readonly string[]): boolean => {
  const ownKeys = Reflect.ownKeys(value);
  if (ownKeys.length !== expectedKeys.length) return false;

  return expectedKeys.every((key) => {
    if (!Object.prototype.hasOwnProperty.call(value, key)) return false;
    const descriptor = Object.getOwnPropertyDescriptor(value, key);
    return descriptor !== undefined && descriptor.enumerable && 'value' in descriptor;
  });
};

const decodeSubaccountIds = (value: unknown): readonly number[] | null => {
  try {
    if (!Array.isArray(value) || Object.getPrototypeOf(value) !== Array.prototype) return null;

    const ownKeys = Reflect.ownKeys(value);
    if (ownKeys.length !== value.length + 1 || !Object.prototype.hasOwnProperty.call(value, 'length')) return null;

    const lengthDescriptor = Object.getOwnPropertyDescriptor(value, 'length');
    if (
      lengthDescriptor === undefined ||
      lengthDescriptor.enumerable ||
      !('value' in lengthDescriptor) ||
      lengthDescriptor.value !== value.length
    ) {
      return null;
    }

    const ids: number[] = [];
    for (let index = 0; index < value.length; index += 1) {
      const key = String(index);
      if (!Object.prototype.hasOwnProperty.call(value, key)) return null;
      const descriptor = Object.getOwnPropertyDescriptor(value, key);
      if (descriptor === undefined || !descriptor.enumerable || !('value' in descriptor)) return null;
      const id = descriptor.value;
      if (typeof id !== 'number' || !Number.isSafeInteger(id) || id <= 0) return null;
      ids.push(id);
    }
    return Object.freeze(ids);
  } catch {
    return null;
  }
};

/**
 * Decode only the published response envelope and account-mode enum. Unknown
 * future enum values remain distinguishable from malformed responses without
 * being treated as Classic.
 */
export const decodeKucoinAccountMode = (input: unknown): KucoinAccountModeDetection => {
  try {
    if (!isPlainRecord(input) || !hasExactEnumerableDataKeys(input, ['code', 'data'])) {
      return { status: 'malformed', reason: 'invalid-envelope' };
    }
    if (input.code !== KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE) {
      return { status: 'malformed', reason: 'invalid-success-code' };
    }

    const data = input.data;
    if (
      !isPlainRecord(data) ||
      !hasExactEnumerableDataKeys(data, ['selfAccountMode', 'unifiedSubAccount', 'classicSubAccount'])
    ) {
      return { status: 'malformed', reason: 'invalid-data' };
    }

    const accountMode = data.selfAccountMode;
    if (typeof accountMode !== 'string' || accountMode.length === 0) {
      return { status: 'malformed', reason: 'invalid-self-account-mode' };
    }
    const unifiedSubAccount = decodeSubaccountIds(data.unifiedSubAccount);
    const classicSubAccount = decodeSubaccountIds(data.classicSubAccount);
    if (unifiedSubAccount === null || classicSubAccount === null) {
      return { status: 'malformed', reason: 'invalid-data' };
    }
    if (accountMode === 'CLASSIC' || accountMode === 'UNIFIED') {
      return Object.freeze({
        status: 'identified',
        accountMode,
        unifiedSubAccount,
        classicSubAccount,
      });
    }
    return { status: 'unknown', reason: 'unknown-self-account-mode' };
  } catch {
    // Reflection over an untrusted Proxy can throw. Detection must still fail
    // closed rather than letting an exotic response escape the boundary.
    return { status: 'malformed', reason: 'invalid-envelope' };
  }
};

/**
 * Pure fail-closed selector for the current Classic-only integrations.
 * KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED is intentionally literal false and is
 * reported for auditability; it never opens an alternate route.
 */
export const decideKucoinClassicRuntime = (
  detection: KucoinAccountModeDetection,
): KucoinClassicRuntimeDecision => {
  if (detection.status === 'identified' && detection.accountMode === 'CLASSIC') {
    return {
      eligible: true,
      reason: 'classic-account-mode-confirmed',
      route: 'existing-classic-runtime',
      utaRuntimeActivationEnabled: KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED,
    };
  }

  const reason =
    detection.status === 'malformed'
      ? 'account-mode-response-malformed'
      : detection.status === 'unknown'
        ? 'account-mode-unknown'
        : 'unified-account-mode-unsupported';

  return {
    eligible: false,
    reason,
    route: 'blocked',
    utaRuntimeActivationEnabled: KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED,
  };
};
