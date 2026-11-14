import type { ExchangeAccountConfig } from '../generated/ExchangeAccountConfig';

import { buildExchangeAccountConfig } from './buildExchangeAccountConfig';

export interface ParseImportResult {
  isValid: boolean;
  data?: ExchangeAccountConfig | ExchangeAccountConfig[];
  error?: string;
}

const isRecord = (value: unknown): value is Record<string, unknown> => typeof value === 'object' && value !== null;

type AccountValidationResult =
  | { ok: true; account: ExchangeAccountConfig }
  | { ok: false; error: string };

/**
 * Backwards-compat: legacy account payloads use a `secret` field instead of
 * `apiSecret`. Mutates the account in place to populate `apiSecret` from
 * `secret` when missing.
 */
function applyApiSecretBackwardCompat(account: unknown): void {
  if (!isRecord(account)) return;
  const obj = account;
  if (!obj.apiSecret && obj.secret) {
    obj.apiSecret = obj.secret;
  }
}

/**
 * Parses import account data (JSON string) for account import
 * Supports both single account objects and arrays of accounts
 *
 * Expected format for single account:
 * {
 *   "exchange": "binance",
 *   "apiKey": "...",
 *   "apiSecret": "...",
 *   "name": "My Account"
 * }
 *
 * Or an array of such objects for multiple accounts
 */
export function parseImportAccountData(rawData: string): ParseImportResult {
  try {
    const parsed: unknown = JSON.parse(rawData);

    // Handle single account object
    if (typeof parsed === 'object' && !Array.isArray(parsed)) {
      applyApiSecretBackwardCompat(parsed);
      const result = validateAccount(parsed, 1);
      if (!result.ok) {
        return { isValid: false, error: result.error };
      }
      return { isValid: true, data: result.account };
    }

    // Handle array of accounts
    if (Array.isArray(parsed)) {
      if (parsed.length === 0) {
        return {
          isValid: false,
          error: 'Empty array - no accounts to import',
        };
      }

      const accounts: ExchangeAccountConfig[] = [];
      for (let i = 0; i < parsed.length; i++) {
        applyApiSecretBackwardCompat(parsed[i]);
        const result = validateAccount(parsed[i], i + 1);
        if (!result.ok) {
          return { isValid: false, error: result.error };
        }
        accounts.push(result.account);
      }

      return { isValid: true, data: accounts };
    }

    return {
      isValid: false,
      error: 'Data must be an account object or array of accounts',
    };
  } catch {
    return {
      isValid: false,
      error: 'Invalid JSON format',
    };
  }
}

/**
 * Validates a single account object
 * Returns error message if invalid, undefined if valid
 */
function validateAccount(account: unknown, index: number): AccountValidationResult {
  if (!isRecord(account)) {
    return { ok: false, error: `Account #${index} is not a valid object` };
  }

  const obj = account;
  const missingFields: string[] = [];

  // Check for required fields - name and exchange are always required
  if (!obj.name) missingFields.push('name');
  if (!obj.exchange) missingFields.push('exchange');

  if (missingFields.length > 0) {
    return {
      ok: false,
      error: `Account #${index} missing fields: ${missingFields.join(', ')}`,
    };
  }

  try {
    return {
      ok: true,
      account: buildExchangeAccountConfig(obj as unknown as Parameters<typeof buildExchangeAccountConfig>[0]),
    };
  } catch (error) {
    return {
      ok: false,
      error: `Account #${index} ${error instanceof Error ? error.message : String(error)}`,
    };
  }
}

/**
 * Validates if an object is a valid ExchangeAccountConfig
 */
export function isValidExchangeAccount(obj: unknown): obj is ExchangeAccountConfig {
  if (typeof obj !== 'object' || obj === null) return false;
  try {
    buildExchangeAccountConfig(obj as unknown as Parameters<typeof buildExchangeAccountConfig>[0]);
    return true;
  } catch {
    return false;
  }
}
