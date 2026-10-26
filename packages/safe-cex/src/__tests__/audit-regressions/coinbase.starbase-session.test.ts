import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { CoinbaseStarbaseProtocolError } from '../reference/coinbase/coinbase.starbase';
import {
  buildCoinbaseStarbaseDisableCancelOnDisconnect,
  buildCoinbaseStarbaseEnableCancelOnDisconnect,
  buildCoinbaseStarbaseGetCancelOnDisconnect,
  COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS,
  decodeCoinbaseStarbaseCancelOnDisconnectMutationResult,
  decodeCoinbaseStarbaseGetCancelOnDisconnectResult,
} from '../reference/coinbase/coinbase.starbase-session';

describe('Coinbase Starbase inert cancel-on-disconnect contracts', () => {
  it('builds the exact three published requests with omitted, connection, or account scope', () => {
    expect(buildCoinbaseStarbaseEnableCancelOnDisconnect()).toEqual({
      method: 'private/enable_cancel_on_disconnect',
      params: {},
    });
    expect(buildCoinbaseStarbaseEnableCancelOnDisconnect('connection')).toEqual({
      method: 'private/enable_cancel_on_disconnect',
      params: { scope: 'connection' },
    });
    expect(buildCoinbaseStarbaseDisableCancelOnDisconnect('account')).toEqual({
      method: 'private/disable_cancel_on_disconnect',
      params: { scope: 'account' },
    });
    expect(buildCoinbaseStarbaseGetCancelOnDisconnect()).toEqual({
      method: 'private/get_cancel_on_disconnect',
      params: {},
    });
    expect(buildCoinbaseStarbaseGetCancelOnDisconnect('account')).toEqual({
      method: 'private/get_cancel_on_disconnect',
      params: { scope: 'account' },
    });
  });

  it('rejects scopes outside the exact published enum', () => {
    expect(() => buildCoinbaseStarbaseEnableCancelOnDisconnect('portfolio' as 'connection')).toThrow(
      CoinbaseStarbaseProtocolError,
    );
    expect(() => buildCoinbaseStarbaseDisableCancelOnDisconnect('' as 'account')).toThrow('cancel-on-disconnect scope');
    expect(() => buildCoinbaseStarbaseGetCancelOnDisconnect(null as unknown as 'account')).toThrow(
      'cancel-on-disconnect scope',
    );
  });

  it('records both mutating methods as WebSocket-only', () => {
    expect(COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS).toEqual([
      'private/enable_cancel_on_disconnect',
      'private/disable_cancel_on_disconnect',
    ]);
    expect(COINBASE_STARBASE_WEBSOCKET_ONLY_SESSION_METHODS).not.toContain('private/get_cancel_on_disconnect');
  });

  it('accepts only the exact ok mutation result', () => {
    expect(decodeCoinbaseStarbaseCancelOnDisconnectMutationResult('ok')).toBe('ok');
    for (const invalid of ['OK', 'ok ', true, null, { result: 'ok' }]) {
      expect(() => decodeCoinbaseStarbaseCancelOnDisconnectMutationResult(invalid)).toThrow(
        CoinbaseStarbaseProtocolError,
      );
    }
  });

  it('strictly decodes the exact enabled and scope result object', () => {
    expect(decodeCoinbaseStarbaseGetCancelOnDisconnectResult({ enabled: false, scope: 'account' })).toEqual({
      enabled: false,
      scope: 'account',
    });
    expect(decodeCoinbaseStarbaseGetCancelOnDisconnectResult({ enabled: true, scope: 'connection' })).toEqual({
      enabled: true,
      scope: 'connection',
    });

    for (const invalid of [
      null,
      [],
      { enabled: true },
      { scope: 'connection' },
      { enabled: 1, scope: 'connection' },
      { enabled: true, scope: 'portfolio' },
      { enabled: true, scope: 'connection', extra: true },
    ]) {
      expect(() => decodeCoinbaseStarbaseGetCancelOnDisconnectResult(invalid)).toThrow(CoinbaseStarbaseProtocolError);
    }
  });

  it('remains statically inert outside the Starbase contract files', () => {
    for (const file of [
      '../../__tests__/reference/coinbase/coinbase.api.ts',
      '../../__tests__/reference/coinbase/coinbase.exchange.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-private.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-public.ts',
      '../../createExchangeFromAccount.ts',
      '../../exchangeMetadata.ts',
      '../../transport/tealstreet/routing/routingTable.ts',
    ]) {
      const source = readFileSync(resolve(__dirname, '../../exchanges/coinbase', file), 'utf8');
      expect(source).not.toContain('coinbase.starbase-session');
    }
  });
});
