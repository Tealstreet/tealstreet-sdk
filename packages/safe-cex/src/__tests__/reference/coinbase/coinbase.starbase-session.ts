/**
 * Inert session-safety contracts for Coinbase's Starbase gateway.
 *
 * This module owns no transport, credentials, routing, feature flag, or side
 * effect. In particular, the enable/disable methods are recorded as
 * WebSocket-only so a future runtime cannot silently route them over HTTP.
 */

import { CoinbaseStarbaseProtocolError } from './coinbase.starbase';

const STARBASE_CANCEL_ON_DISCONNECT_SCOPES = ['connection', 'account'] as const;

export type CoinbaseStarbaseCancelOnDisconnectScope = (typeof STARBASE_CANCEL_ON_DISCONNECT_SCOPES)[number];

export type CoinbaseStarbaseSessionRequest<Method extends string, Params extends Record<string, unknown>> = {
  method: Method;
  params: Params;
};

export type CoinbaseStarbaseCancelOnDisconnectParams = {
  scope?: CoinbaseStarbaseCancelOnDisconnectScope;
};

export type CoinbaseStarbaseCancelOnDisconnectResult = {
  enabled: boolean;
  scope: CoinbaseStarbaseCancelOnDisconnectScope;
};

export const COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS = Object.freeze([
  'private/enable_cancel_on_disconnect',
  'private/disable_cancel_on_disconnect',
] as const);

const protocolError = (message: string): never => {
  throw new CoinbaseStarbaseProtocolError(message);
};

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

const parseScope = (value: unknown): CoinbaseStarbaseCancelOnDisconnectScope => {
  if (
    typeof value !== 'string' ||
    !STARBASE_CANCEL_ON_DISCONNECT_SCOPES.includes(value as CoinbaseStarbaseCancelOnDisconnectScope)
  ) {
    return protocolError('Invalid Starbase cancel-on-disconnect scope');
  }
  return value as CoinbaseStarbaseCancelOnDisconnectScope;
};

const buildScopeParams = (scope?: CoinbaseStarbaseCancelOnDisconnectScope): CoinbaseStarbaseCancelOnDisconnectParams =>
  scope === undefined ? {} : { scope: parseScope(scope) };

export const buildCoinbaseStarbaseEnableCancelOnDisconnect = (
  scope?: CoinbaseStarbaseCancelOnDisconnectScope,
): CoinbaseStarbaseSessionRequest<'private/enable_cancel_on_disconnect', CoinbaseStarbaseCancelOnDisconnectParams> => ({
  method: 'private/enable_cancel_on_disconnect',
  params: buildScopeParams(scope),
});

export const buildCoinbaseStarbaseDisableCancelOnDisconnect = (
  scope?: CoinbaseStarbaseCancelOnDisconnectScope,
): CoinbaseStarbaseSessionRequest<
  'private/disable_cancel_on_disconnect',
  CoinbaseStarbaseCancelOnDisconnectParams
> => ({
  method: 'private/disable_cancel_on_disconnect',
  params: buildScopeParams(scope),
});

export const buildCoinbaseStarbaseGetCancelOnDisconnect = (
  scope?: CoinbaseStarbaseCancelOnDisconnectScope,
): CoinbaseStarbaseSessionRequest<'private/get_cancel_on_disconnect', CoinbaseStarbaseCancelOnDisconnectParams> => ({
  method: 'private/get_cancel_on_disconnect',
  params: buildScopeParams(scope),
});

export const decodeCoinbaseStarbaseCancelOnDisconnectMutationResult = (value: unknown): 'ok' => {
  if (value !== 'ok') {
    return protocolError('Invalid Starbase cancel-on-disconnect mutation result');
  }
  return value;
};

export const decodeCoinbaseStarbaseGetCancelOnDisconnectResult = (
  value: unknown,
): CoinbaseStarbaseCancelOnDisconnectResult => {
  if (!isRecord(value)) {
    return protocolError('Invalid Starbase cancel-on-disconnect result: expected an object');
  }

  const keys = Object.keys(value).sort();
  if (keys.length !== 2 || keys[0] !== 'enabled' || keys[1] !== 'scope') {
    return protocolError('Invalid Starbase cancel-on-disconnect result: expected exact enabled and scope fields');
  }
  if (typeof value.enabled !== 'boolean') {
    return protocolError('Invalid Starbase cancel-on-disconnect enabled value');
  }

  return {
    enabled: value.enabled,
    scope: parseScope(value.scope),
  };
};
