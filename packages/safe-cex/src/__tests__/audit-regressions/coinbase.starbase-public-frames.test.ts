import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, expect, it } from 'vitest';

import { CoinbaseStarbaseProtocolError } from '../reference/coinbase/coinbase.starbase';
import {
  buildCoinbaseStarbaseTickerChannel,
  normalizeCoinbaseStarbaseTickerPayload,
} from '../reference/coinbase/coinbase.starbase-public';
import {
  classifyCoinbaseStarbasePublicWebsocketFrame,
  COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256,
  COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED,
} from '../reference/coinbase/coinbase.starbase-public-frames';
import {
  STARBASE_CAPTURE_DERIVED_HEARTBEAT_FRAME,
  STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME,
  STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_ERROR_FRAME,
  STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_RESULT_FRAME,
  STARBASE_CAPTURE_DERIVED_TEST_REQUEST_FRAME,
} from './coinbase.starbase-public-frames.fixtures';

const tickerChannel = buildCoinbaseStarbaseTickerChannel('BTC-PERP-INTX');
const context = {
  acknowledgedPublicChannels: [tickerChannel],
  desiredPublicChannels: [tickerChannel],
  pendingPublicTestRequestIds: [41, 42],
} as const;

describe('Coinbase Starbase inert public WebSocket frame classifier', () => {
  it('pins the independently captured public envelope evidence without activating it', () => {
    expect(COINBASE_STARBASE_PUBLIC_FRAME_CAPTURE_SHA256).toEqual({
      heartbeat: '7a63af8955274769a006d4dd7ab448ff7f2c3a255a11871b2a95e9bb9e00e503',
      notification: 'a18e355e8c29e6a5699166e5dd5e67cc797345d6fda6f6e30676a06d91dba692',
      testRequest: '01bee32b135c20dea365212880829f7c34ad4eabe00bd1f13a8d0dcec544280a',
    });
    expect(COINBASE_STARBASE_PUBLIC_FRAME_CLASSIFIER_ACTIVATION_ENABLED).toBe(false);
  });

  it('classifies only an exact acknowledged public subscription and passes the params boundary', () => {
    const classified = classifyCoinbaseStarbasePublicWebsocketFrame(
      STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME,
      context,
    );
    expect(classified.kind).toBe('public-subscription');
    if (classified.kind !== 'public-subscription') throw new Error('Expected a public subscription fixture');

    expect(classified.channel).toBe(tickerChannel);
    expect(classified.payload.channel).toBe(tickerChannel);
    expect(normalizeCoinbaseStarbaseTickerPayload(classified.payload).instrumentName).toBe('BTC_USDC-PERPETUAL');
  });

  it('leaves every non-acknowledged subscription explicitly unsupported and unclassified', () => {
    for (const channel of ['ticker.ETH_USDC-PERPETUAL.100ms', 'user.orders.BTC_USDC-PERPETUAL.raw']) {
      const classified = classifyCoinbaseStarbasePublicWebsocketFrame(
        JSON.stringify({
          jsonrpc: '2.0',
          method: 'subscription',
          params: { channel, data: { deliberately: 'opaque' } },
        }),
        context,
      );
      expect(classified).toEqual({
        channel,
        kind: 'unsupported-subscription',
        reason: 'channel-not-a-desired-acknowledged-public-subscription',
        scope: 'unclassified',
      });
    }

    for (const partialContext of [
      { acknowledgedPublicChannels: [], desiredPublicChannels: [tickerChannel] },
      { acknowledgedPublicChannels: [tickerChannel], desiredPublicChannels: [] },
    ] as const) {
      expect(
        classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME, {
          ...partialContext,
          pendingPublicTestRequestIds: [],
        }),
      ).toMatchObject({ kind: 'unsupported-subscription', scope: 'unclassified' });
    }
  });

  it('requires captured notification discriminants and forbids id/result fields', () => {
    const base = JSON.parse(STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME) as Record<string, unknown>;
    for (const frame of [
      { ...base, id: 1 },
      { ...base, result: 'ok' },
      { ...base, method: 'notifications' },
      { ...base, params: { channel: tickerChannel } },
      { ...base, params: { channel: tickerChannel, data: {}, extra: true } },
    ]) {
      expect(classifyCoinbaseStarbasePublicWebsocketFrame(JSON.stringify(frame), context)).toEqual({
        kind: 'unknown',
        reason: 'invalid-public-websocket-envelope',
      });
    }
  });

  it('classifies only the captured heartbeat and test_request union without an id', () => {
    expect(classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_HEARTBEAT_FRAME, context)).toEqual({
      kind: 'heartbeat',
      signal: 'heartbeat',
    });
    expect(classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_TEST_REQUEST_FRAME, context)).toEqual({
      kind: 'heartbeat',
      signal: 'test_request',
    });

    for (const frame of [
      { jsonrpc: '2.0', method: 'heartbeat', params: { type: 'ping' } },
      { jsonrpc: '2.0', id: 1, method: 'heartbeat', params: { type: 'heartbeat' } },
      { jsonrpc: '2.0', method: 'heartbeat', params: { type: 'heartbeat', sequence: 1 } },
    ]) {
      expect(classifyCoinbaseStarbasePublicWebsocketFrame(JSON.stringify(frame), context).kind).toBe('unknown');
    }
  });

  it('accepts only known positive public/test IDs with exactly one valid result or error', () => {
    expect(
      classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_RESULT_FRAME, context),
    ).toEqual({
      id: 41,
      kind: 'public-test-response',
      outcome: 'result',
      result: { version: '1.2.26' },
    });
    expect(
      classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_ERROR_FRAME, context),
    ).toEqual({
      error: { code: -32601, data: 'Method not found', message: 'Method not found' },
      id: 42,
      kind: 'public-test-response',
      outcome: 'error',
    });

    for (const frame of [
      { jsonrpc: '2.0', id: 99, result: { version: '1.2.26' } },
      { jsonrpc: '2.0', id: 41, result: { version: '1.2.26' }, error: { code: 1, message: 'both' } },
      { jsonrpc: '2.0', id: 41 },
      { jsonrpc: '2.0', id: 41, result: { version: '' } },
      { jsonrpc: '2.0', id: 41, result: { version: '1.2.26' }, method: 'public/test' },
    ]) {
      expect(classifyCoinbaseStarbasePublicWebsocketFrame(JSON.stringify(frame), context).kind).toBe('unknown');
    }
  });

  it('allows only proven public/test metadata keys with validated timing domains', () => {
    for (const key of ['arbitrary', '__proto__', 'constructor', 'prototype']) {
      const raw = `{"jsonrpc":"2.0","id":41,"result":{"version":"1.2.26"},"${key}":true}`;
      expect(classifyCoinbaseStarbasePublicWebsocketFrame(raw, context)).toEqual({
        kind: 'unknown',
        reason: 'invalid-public-test-response',
      });
    }

    for (const metadata of [
      { testnet: 'false' },
      { usIn: 0 },
      { usOut: -1 },
      { usDiff: -1 },
      { usDiff: 1.5 },
      { usIn: 100, usOut: 99 },
      { usDiff: 2, usIn: 100, usOut: 101 },
    ]) {
      expect(
        classifyCoinbaseStarbasePublicWebsocketFrame(
          JSON.stringify({ jsonrpc: '2.0', id: 41, result: { version: '1.2.26' }, ...metadata }),
          context,
        ),
      ).toEqual({ kind: 'unknown', reason: 'invalid-public-test-response' });
    }
  });

  it('requires public/test error codes to be safe integers', () => {
    expect(
      classifyCoinbaseStarbasePublicWebsocketFrame(
        '{"jsonrpc":"2.0","id":42,"error":{"code":-32601,"message":"Method not found"}}',
        context,
      ),
    ).toMatchObject({ error: { code: -32601 }, kind: 'public-test-response', outcome: 'error' });

    for (const code of ['9007199254740992', '9.007199254740992e15', '1e30']) {
      expect(
        classifyCoinbaseStarbasePublicWebsocketFrame(
          `{"jsonrpc":"2.0","id":42,"error":{"code":${code},"message":"unsafe"}}`,
          context,
        ),
      ).toEqual({ kind: 'unknown', reason: 'invalid-public-test-response' });
    }
  });

  it('rejects duplicate JSON keys before parsing, including nested and escaped-equivalent keys', () => {
    for (const raw of [
      '{"jsonrpc":"2.0","id":41,"id":42,"result":{"version":"1.2.26"}}',
      '{"jsonrpc":"2.0","id":41,"result":{"version":"1.2.26","version":"2"}}',
      '{"jsonrpc":"2.0","id":41,"\\u0069d":42,"result":{"version":"1.2.26"}}',
      '{"jsonrpc":"2.0","id":41,"result":{"version":"1.2.26","ver\\u0073ion":"2"}}',
      '{"jsonrpc":"2.0","id":42,"error":{"code":1,"message":"x","data":{"key":1,"k\\u0065y":2}}}',
    ]) {
      expect(classifyCoinbaseStarbasePublicWebsocketFrame(raw, context)).toEqual({
        kind: 'unknown',
        reason: 'duplicate-json-key',
      });
    }
  });

  it('scans arrays and escaped strings without treating keys in separate objects as duplicates', () => {
    const raw = JSON.stringify({
      jsonrpc: '2.0',
      id: 42,
      error: {
        code: -32601,
        message: 'Method {"not":"a key"}',
        data: [{ same: 1 }, { same: [true, false, null, 'escaped \\" quote'] }],
      },
    });
    expect(classifyCoinbaseStarbasePublicWebsocketFrame(raw, context)).toMatchObject({
      kind: 'public-test-response',
      outcome: 'error',
    });
  });

  it('returns safe unknown classifications for malformed or unsupported raw frames', () => {
    expect(classifyCoinbaseStarbasePublicWebsocketFrame('{', context)).toEqual({
      kind: 'unknown',
      reason: 'invalid-json',
    });
    expect(classifyCoinbaseStarbasePublicWebsocketFrame('[]', context)).toEqual({
      kind: 'unknown',
      reason: 'invalid-public-websocket-envelope',
    });
    expect(classifyCoinbaseStarbasePublicWebsocketFrame(new Uint8Array(), context)).toEqual({
      kind: 'unknown',
      reason: 'unsupported-raw-frame',
    });
  });

  it('rejects invalid classifier context rather than broadening channel or request correlation', () => {
    expect(() =>
      classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME, {
        acknowledgedPublicChannels: [tickerChannel, tickerChannel],
        desiredPublicChannels: [tickerChannel],
        pendingPublicTestRequestIds: [],
      }),
    ).toThrow(CoinbaseStarbaseProtocolError);
    expect(() =>
      classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_RESULT_FRAME, {
        acknowledgedPublicChannels: [],
        desiredPublicChannels: [],
        pendingPublicTestRequestIds: [0],
      }),
    ).toThrow(CoinbaseStarbaseProtocolError);
    expect(() =>
      classifyCoinbaseStarbasePublicWebsocketFrame(STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME, {
        acknowledgedPublicChannels: ['user.orders.BTC_USDC-PERPETUAL.raw' as typeof tickerChannel],
        desiredPublicChannels: ['user.orders.BTC_USDC-PERPETUAL.raw' as typeof tickerChannel],
        pendingPublicTestRequestIds: [],
      }),
    ).toThrow('Invalid acknowledged Starbase public channels');
  });

  it('remains absent from active Coinbase runtime, metadata, registry, and routing', () => {
    for (const file of [
      '../../__tests__/reference/coinbase/coinbase.api.ts',
      '../../__tests__/reference/coinbase/coinbase.exchange.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-private.ts',
      '../../__tests__/reference/coinbase/coinbase.ws-public.ts',
      '../../createExchangeFromAccount.ts',
      '../../exchangeMetadata.ts',
      '../registry.native.ts',
      '../registry.ts',
      '../../transport/tealstreet/routing/routingTable.ts',
    ]) {
      const source = readFileSync(resolve(__dirname, '../../exchanges/coinbase', file), 'utf8');
      expect(source).not.toContain('coinbase.starbase-public-frames');
    }
  });
});
