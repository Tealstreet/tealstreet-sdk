// @vitest-environment node
import type { AccountAuthResult, Transport, WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeOptions } from '../../types';

import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { nativeTransport } from '@tealstreet/safe-cex-wasm/host';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { setTickIntervalMultipliers } from '../../utils/tickIntervalOverride';
import { consumerFrame } from '../cutover/consumerFrame';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { nativeReadyFrame } from './rust.native-startup.fixtures';

const accounts: RustEngineExchange[] = [];
const auth = (
  credential: AccountAuthResult['credential'],
  scope: AccountAuthResult['scope'],
  outcome: AccountAuthResult['outcome'],
) => ({ op: 'accountAuth', credential, scope, outcome });

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  accounts.splice(0).forEach((exchange) => exchange.dispose());
  setTickIntervalMultipliers(undefined);
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(nativeTransport);
  vi.useRealTimers();
  vi.restoreAllMocks();
});

function fixture(options: Partial<ExchangeOptions> = {}, initial: unknown[] = []) {
  let inbound: unknown[] = [];
  let socket!: Parameters<Transport['connect']>[1];
  let failNextPolicy = false;
  let trapMessages = false;
  const commands: Array<{ op: string; scale?: number | null; ceiling?: number }> = [];
  const idle = () => '[]';
  const native: WasmEngineLike = {
    capabilities: () => engineCapabilities('binance'),
    start: () => JSON.stringify([{ op: 'wsConnect', socket: 1, url: 'wss://controlled.invalid/private' }, ...initial]),
    command(json) {
      const command = JSON.parse(json);
      commands.push(command);
      if (command.op === 'setPollScale' && failNextPolicy) {
        failNextPolicy = false;
        throw new Error('Controlled policy failure');
      }
      return command.command === undefined
        ? '[]'
        : JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
    },
    takeFrame: nativeReadyFrame,
    snapshotFrame: () => new Uint8Array(consumerFrame({ markets: [], tickers: [] })),
    wsMessage: () => {
      if (trapMessages) throw new WebAssembly.RuntimeError('unreachable');
      return JSON.stringify(inbound.splice(0));
    },
    wsOpen: idle,
    wsClose: idle, wsRouteExhausted: idle,
    httpStart: idle,
    httpChunk: idle,
    httpEnd: idle,
    httpFailed: idle,
    timer: idle,
  };
  setWasmEngineLoader(async () => ({
    WasmEngine: function () {
      return native;
    } as unknown as WasmEngineModule['WasmEngine'],
  }));
  const request = vi
    .fn<Transport['request']>()
    .mockResolvedValue({ status: 401, body: new TextEncoder().encode('{"error":"invalid key"}') });
  setRustEngineTransport(() => ({
    now: Date.now,
    connect(_url, callbacks) {
      socket = callbacks;
      return { send() {}, close() {} };
    },
    request,
    setTimeout,
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  }));
  const exchange = new RustEngineExchange(
    'binance',
    {
      key: 'primary',
      secret: 'primary-secret',
      publicDataOnly: false,
      secondary_key: 'read',
      secondary_secret: 'read-secret',
      extra: { tickInterval: 1000 },
      tuning: { consoleLogging: false },
      ...options,
    },
    new DefaultStore(),
  );
  accounts.push(exchange);
  const errors = vi.fn();
  const deactivated = vi.fn();
  exchange.on('error', errors);
  exchange.on('deactivate', deactivated);
  return {
    exchange,
    native,
    errors,
    deactivated,
    request,
    commands,
    // The resolved absolute tick travels as the scale's ceiling; null pauses.
    controls: () => commands.filter((command) => command.op === 'setPollScale').map((command) => (command.scale === null ? null : command.ceiling)),
    start: async () => {
      const starting = exchange.start();
      await vi.advanceTimersByTimeAsync(250);
      await starting;
    },
    deliver: (...actions: unknown[]) => {
      inbound = actions;
      socket.onMessage('controlled-native-action');
    },
    failPolicy: () => {
      failNextPolicy = true;
    },
    trapMessages: () => { trapMessages = true; },
  };
}

it('native startup rejection changes polling without inventing a terminal error', async () => {
  const r = fixture({}, [auth('secondary', 'orders', 'rejected')]);
  await r.start();
  expect(r.controls()).toEqual([1000, 2000]);
  expect(r.exchange.authFailureStreak).toBe(1);
  expect(r.errors).not.toHaveBeenCalled();
  expect(r.deactivated).not.toHaveBeenCalled();
  expect(r.exchange.isDisposed).toBe(false);
});

it('isolates all ten signer/scope contexts and clears only matching accepted evidence', async () => {
  const r = fixture();
  await r.start();
  const contexts = (['primary', 'secondary'] as const).flatMap((credential) =>
    (['account', 'balance', 'orders', 'positions', 'settings'] as const).map((scope) => ({ credential, scope })),
  );
  for (const { credential, scope } of contexts) r.deliver(auth(credential, scope, 'rejected'));
  expect(r.controls()).toEqual([1000, 2000]);
  for (const { credential, scope } of contexts.slice(0, -1)) {
    r.deliver(auth(credential, scope, 'accepted'));
    expect(r.exchange.authFailureStreak).toBe(1);
  }
  r.deliver(auth('secondary', 'settings', 'accepted'));
  expect(r.controls()).toEqual([1000, 2000, 1000]);
  expect(r.exchange.authFailureStreak).toBe(0);
  expect(r.errors).not.toHaveBeenCalled();
});

it('preserves a rejected orders permission across balance/login/other-signer successes and legacy manual calls', async () => {
  const r = fixture();
  await r.start();
  for (let i = 0; i < 3; i++) r.deliver(auth('secondary', 'orders', 'rejected'));
  r.deliver(
    auth('secondary', 'balance', 'accepted'),
    auth('secondary', 'account', 'accepted'),
    auth('primary', 'orders', 'accepted'),
  );
  r.exchange.recordAuthFailure();
  r.exchange.recordAuthSuccess();
  expect(r.exchange.authFailureStreak).toBe(3);
  expect(r.controls()).toEqual([1000, 2000, 4000, 8000]);
  r.deliver(auth('secondary', 'orders', 'accepted'));
  expect(r.controls().at(-1)).toBe(1000);
});

it('caps scoped streaks and composes the server multiplier while preserving Infinity pause/resume', async () => {
  const r = fixture();
  await r.start();
  setTickIntervalMultipliers({ '*': 3 });
  for (let i = 0; i < 20; i++) r.deliver(auth('secondary', 'positions', 'rejected'));
  expect(r.exchange.authFailureStreak).toBe(7);
  expect(r.controls().at(-1)).toBe(60_000);
  const cappedCount = r.controls().length;
  r.deliver(auth('secondary', 'positions', 'rejected'));
  expect(r.controls()).toHaveLength(cappedCount);
  r.exchange.options.extra!.tickInterval = Infinity;
  await vi.advanceTimersByTimeAsync(250);
  r.deliver(auth('secondary', 'positions', 'accepted'), auth('primary', 'settings', 'rejected'));
  expect(r.controls().at(-1)).toBeNull();
  const pausedCount = r.controls().length;
  r.deliver(auth('primary', 'settings', 'rejected'));
  expect(r.controls()).toHaveLength(pausedCount);
  r.exchange.options.extra!.tickInterval = 1000;
  await vi.advanceTimersByTimeAsync(250);
  expect(r.controls().at(-1)).toBe(12_000);
});

it('keeps failed policy delivery retryable without adding writes or terminal state', async () => {
  const r = fixture();
  await r.start();
  r.failPolicy();
  r.deliver(auth('secondary', 'orders', 'rejected'));
  await vi.advanceTimersByTimeAsync(0);
  expect(r.errors).toHaveBeenCalledOnce();
  expect(r.exchange.authFailureStreak).toBe(1);
  await vi.advanceTimersByTimeAsync(250);
  expect(r.controls()).toEqual([1000, 2000, 2000]);
  expect(r.commands.every(({ op }) => ['setPollScale', 'setTickerSymbols'].includes(op))).toBe(true);
  expect(r.exchange.isDisposed).toBe(false);
});

it('does not classify HTTP status/body or background logs in JavaScript', async () => {
  const r = fixture();
  await r.start();
  r.deliver(
    { op: 'event', level: 'warn', code: 'request.failed', message: 'invalid API key', fields: {} },
    { op: 'http', request: 3, method: 'GET', url: 'https://api.binance.com/private', headers: [] },
  );
  await vi.advanceTimersByTimeAsync(0);
  expect(r.request).toHaveBeenCalledOnce();
  expect(r.controls()).toEqual([1000]);
  expect(r.exchange.authFailureStreak).toBe(0);
});

it('fences disposed and dead generations while sibling and replacement accounts remain independent', async () => {
  const old = fixture();
  await old.start();
  const sibling = fixture();
  await sibling.start();
  old.deliver(auth('secondary', 'orders', 'rejected'));
  expect(sibling.exchange.authFailureStreak).toBe(0);
  old.exchange.dispose();
  old.deliver(auth('secondary', 'orders', 'rejected'));
  expect(old.exchange.authFailureStreak).toBe(1);
  const replacement = fixture();
  await replacement.start();
  expect(replacement.controls()).toEqual([1000]);
  sibling.trapMessages();
  sibling.deliver();
  sibling.deliver(auth('secondary', 'orders', 'rejected'));
  expect(sibling.exchange.authFailureStreak).toBe(0);
  expect(replacement.exchange.authFailureStreak).toBe(0);
});

it('a scoped account GET refusal and its error diagnostic recover without a phantom primary context', async () => {
  const r = fixture();
  await r.start();
  r.deliver(
    auth('secondary', 'positions', 'rejected'),
    { op: 'event', level: 'error', code: 'request.failed', message: 'Bitunix account read failed: code 10004', fields: { venueCode: '10004' } },
  );
  expect(r.exchange.authFailureStreak).toBe(1);
  expect(r.errors).toHaveBeenCalledOnce();
  r.deliver(auth('secondary', 'positions', 'accepted'));
  expect(r.exchange.authFailureStreak).toBe(0);
  expect(r.deactivated).not.toHaveBeenCalled();
  expect(r.exchange.isDisposed).toBe(false);
});
