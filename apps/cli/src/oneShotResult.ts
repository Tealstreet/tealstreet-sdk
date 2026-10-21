import type { PrintKind } from './listener/adapter-context';
import type { AuthState } from './auth';
import { classifyConnectError } from './connect-error';

/**
 * Stable codes an agent branches on. Each names the step that fixes it:
 * install/login/account add/key/network — see the CLI's agent guide.
 */
export type OneShotCode =
  | 'OK'
  | 'COMMAND_FAILED'
  | 'NO_ACCOUNTS'
  | 'ACCOUNT_NOT_FOUND'
  | 'NOT_LOGGED_IN'
  | 'AUTH_EXPIRED'
  | 'KEY_REJECTED'
  | 'CONNECTION_TIMEOUT'
  | 'CONNECTION_FAILED'
  | 'UNCONFIRMED'
  | 'USAGE_ERROR'
  | 'INTERNAL_ERROR';

export type OneShotLine = { kind: PrintKind; text: string };

export type OneShotResult = {
  ok: boolean;
  code: OneShotCode;
  command: string;
  account?: string;
  symbol?: string;
  error?: string;
  output: OneShotLine[];
};

// eslint-disable-next-line no-control-regex
const ANSI = /\u001b\[[0-9;]*m/g;
export const stripAnsi = (text: string) => text.replace(ANSI, '');

// What venues say when they refuse a key. Bybit answers HTTP 200 and only logs the
// rejection, so the run would otherwise print a zeroed balance and look healthy.
const KEY_REJECTION = new RegExp(
  [
    'private API was rejected', // bybit
    'api[ -]?key (is )?invalid',
    'invalid api[ -]?key', // binance "Invalid API-key, IP, or permissions"
    "api[ -]?key.*(expired|not found|doesn't exist|does not exist)", // okx
    'invalid (ok-access-key|sign)', // okx
    'invalid signature',
    'signature (mismatch|invalid|for this request is not valid)', // binance
    'credentials were rejected', // Rust engine venues
  ].join('|'),
  'i',
);

/** The first exchange-reported line that means the venue refused the account's key. */
export const findKeyRejection = (exchangeLines: readonly string[]) =>
  exchangeLines.find((line) => KEY_REJECTION.test(line));

/**
 * The command's own error lines or a throw mean it failed. An exchange error
 * logged while the command reported success is UNCONFIRMED: a venue can reject
 * an order and only log it, but it also logs errors it then handles, so the
 * caller must look at positions and orders rather than retry blind.
 */
export const summarizeOneShot = (
  commandOutput: OneShotLine[],
  threw?: unknown,
  exchangeOutput: OneShotLine[] = [],
): { ok: boolean; code: OneShotCode; error?: string } => {
  if (threw !== undefined) {
    return { ok: false, code: 'COMMAND_FAILED', error: threw instanceof Error ? threw.message : String(threw) };
  }
  const commandError = commandOutput.find((line) => line.kind === 'error')?.text;
  if (commandError) return { ok: false, code: 'COMMAND_FAILED', error: commandError };
  const exchangeError = exchangeOutput.find((line) => line.kind === 'error')?.text;
  return exchangeError ? { ok: false, code: 'UNCONFIRMED', error: exchangeError } : { ok: true, code: 'OK' };
};

/**
 * `connectToAccount` reports a failure by printing it, not by throwing, so the
 * code comes from the printed line. An auth failure is a Tealstreet sign-in
 * problem (proxy/tunnel routing), split by whether the user ever signed in.
 */
export const connectFailureCode = (errorLine: string | undefined, auth: AuthState): OneShotCode => {
  const kind = classifyConnectError(errorLine ?? '');
  if (kind === 'auth') return auth.state === 'not_logged_in' ? 'NOT_LOGGED_IN' : 'AUTH_EXPIRED';
  if (kind === 'timeout') return 'CONNECTION_TIMEOUT';
  return 'CONNECTION_FAILED';
};
