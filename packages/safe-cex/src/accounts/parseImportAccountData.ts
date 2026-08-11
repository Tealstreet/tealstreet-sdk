import type { ExchangeAccountConfig } from '../types';

import { buildExchangeAccountConfig } from './buildExchangeAccountConfig';

export interface ParseImportResult {
  isValid: boolean;
  data?: ExchangeAccountConfig | ExchangeAccountConfig[];
  error?: string;
}

/**
 * Backwards-compat: legacy account payloads use a `secret` field instead of
 * `apiSecret`. Mutates the account in place to populate `apiSecret` from
 * `secret` when missing.
 */
function applyApiSecretBackwardCompat(account: unknown): void {
  if (typeof account !== 'object' || account === null) return;
  const obj = account as Record<string, unknown>;
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
    const parsed = JSON.parse(rawData);

    // Handle single account object
    if (typeof parsed === 'object' && !Array.isArray(parsed)) {
      applyApiSecretBackwardCompat(parsed);
      const result = validateAccount(parsed, 1);
      if (result.error) {
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

      for (let i = 0; i < parsed.length; i++) {
        applyApiSecretBackwardCompat(parsed[i]);
        const result = validateAccount(parsed[i], i + 1);
        if (result.error) {
          return { isValid: false, error: result.error };
        }
        parsed[i] = result.account;
      }

      return { isValid: true, data: parsed as ExchangeAccountConfig[] };
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
function validateAccount(account: unknown, index: number): { account?: ExchangeAccountConfig; error?: string } {
  if (typeof account !== 'object' || account === null) {
    return { error: `Account #${index} is not a valid object` };
  }

  const obj = account as Record<string, unknown>;
  const missingFields: string[] = [];

  // Check for required fields - name and exchange are always required
  if (!obj.name) missingFields.push('name');
  if (!obj.exchange) missingFields.push('exchange');

  if (missingFields.length > 0) {
    return {
      error: `Account #${index} missing fields: ${missingFields.join(', ')}`,
    };
  }

  try {
    return {
      account: buildExchangeAccountConfig(obj as unknown as Parameters<typeof buildExchangeAccountConfig>[0]),
    };
  } catch (error) {
    return {
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
