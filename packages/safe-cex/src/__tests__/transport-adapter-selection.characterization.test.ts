import type { AxiosInstance } from 'axios';
import type { BaseExchange } from '../exchanges/base';
import type { ExchangeName, ExchangeOptions } from '../types';

import { afterAll, afterEach, describe, expect, it, vi } from 'vitest';

import '../transport/registerTealstreetTransport';

import { exchangeRegistry } from '../exchanges/registry';
import { RustEngineExchange } from '../exchanges/rust/rust.exchange';
import {
  axiosHttpOverWebsocketAdapter,
  bigIntAxiosHttpOverWebsocketAdapter,
  disposeHOWWebSockets,
} from '../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { DefaultStore } from '../store/store.base';

// Inspect REST adapter construction, without bootstrapping a remote SDK account.
vi.mock('../exchanges/paradex/paradex.sdk', async (original) => ({
  ...await original<typeof import('../exchanges/paradex/paradex.sdk')>(),
  initializeParadexSDK: vi.fn(async () => null),
}));

vi.hoisted(() => {
  // HyroTrader builds Bybit REST clients during construction, before the test ever starts I/O.
  process.env.NEXT_PUBLIC_HYROTRADER_PROD_REST_URL ??= 'https://hyrotrader-prod.test';
});

// Tag every adapter built after module load with the region it was asked for. The
// package's own singletons are created before this wrapper and are matched by identity.
vi.mock('@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter', async (importOriginal) => {
  const original =
    await importOriginal<typeof import('@tealstreet/http-over-websocket/src/axiosHttpOverWebsocketAdapter')>();
  return {
    ...original,
    createAxiosHttpOverWebsocketAdapter: (options?: unknown) => {
      const adapter = original.createAxiosHttpOverWebsocketAdapter(options as never);
      const opts =
        typeof options === 'object' && options ? (options as { region?: string; customParser?: unknown }) : {};
      const parser = typeof options === 'function' || opts.customParser ? ':parser' : '';
      return Object.assign(adapter, { __howLabel: `how:${opts.region ?? 'byBaseURL'}${parser}` });
    },
  };
});

/**
 * Pins which axios adapter and baseURL every exchange's REST clients get for each
 * transport option combination. The routing golden master pins hosts; this pins
 * the per-factory adapter choice, which the TransportProvider refactor moves.
 *
 * `remoteSigner` is deliberately unset: with it, `.defaults.adapter` holds the
 * remote-signing wrapper and the selected adapter hides in a private WeakMap.
 */

const TRANSPORT_COMBOS: Record<string, Partial<ExchangeOptions>> = {
  direct: {},
  'proxy-SG': { tealstreetProxy: true, proxyRegion: 'SG' },
  'proxy-JA': { tealstreetProxy: true, proxyRegion: 'JA' },
  'proxy+tunnel-SG': { tealstreetProxy: true, tealstreetTunnel: true, proxyRegion: 'SG' },
  'proxy+tunnel-JA': { tealstreetProxy: true, tealstreetTunnel: true, proxyRegion: 'JA' },
  'tunnel-only': { tealstreetTunnel: true },
};

const BASE_OPTIONS: ExchangeOptions = {
  key: 'transport-contract-key',
  secret: 'transport-contract-secret',
  passphrase: 'transport-contract-passphrase',
  applicationId: 'transport-contract-application',
  apiKeyIndex: '0',
  starkPrivateKey: '0x1',
  paradexAddress: '0x1',
  subkeyAddress: '0x1',
  grvtSignerAddress: '0x0000000000000000000000000000000000000001',
  grvtTradingAccountId: 'transport-contract-account',
};

const liveInstances: BaseExchange[] = [];

afterEach(() => {
  liveInstances.splice(0).forEach((exchange) => exchange.dispose());
});

afterAll(() => {
  disposeHOWWebSockets();
});

// An axios instance is a callable function carrying `defaults` and `interceptors`.
const isAxiosInstance = (value: unknown): value is AxiosInstance =>
  (typeof value === 'function' || typeof value === 'object') &&
  value !== null &&
  typeof (value as AxiosInstance).defaults === 'object' &&
  typeof (value as AxiosInstance).interceptors === 'object';

const labelAdapter = (adapter: unknown): string => {
  if (adapter === undefined || Array.isArray(adapter)) return 'default';
  if (adapter === axiosHttpOverWebsocketAdapter) return 'how';
  if (adapter === bigIntAxiosHttpOverWebsocketAdapter) return 'how:bigInt';
  const tagged = (adapter as { __howLabel?: string }).__howLabel;
  if (tagged) return tagged;
  return `custom:${(adapter as { name?: string }).name || 'anonymous'}`;
};

/** Axios instances on the exchange and one object below it, grouped by identity. */
const collectAxiosInstances = (exchange: BaseExchange): Record<string, string> => {
  const found = new Map<AxiosInstance, string[]>();
  const visit = (owner: object, prefix: string, depth: number) => {
    for (const [key, value] of Object.entries(owner)) {
      if (isAxiosInstance(value)) {
        found.set(value, [...(found.get(value) ?? []), `${prefix}${key}`]);
      } else if (depth > 0 && typeof value === 'object' && value !== null && !Array.isArray(value)) {
        visit(value, `${prefix}${key}.`, depth - 1);
      }
    }
  };
  visit(exchange, '', 1);

  const out: Record<string, string> = {};
  for (const [instance, names] of found) {
    out[names.sort().join(' = ')] = `${labelAdapter(instance.defaults.adapter)} ${instance.defaults.baseURL ?? ''}`;
  }
  return Object.fromEntries(Object.entries(out).sort(([a], [b]) => a.localeCompare(b)));
};

const ADAPTER_SELECTION_TIMEOUT_MS = 120_000;

describe('transport adapter selection per exchange', () => {
  const exchangeNames = Object.keys(exchangeRegistry).sort() as ExchangeName[];

  it.each(exchangeNames)(
    '%s',
    async (exchangeName) => {
      const Exchange = await exchangeRegistry[exchangeName]();
      const selection: Record<string, Record<string, string>> = {};

      for (const publicDataOnly of [false, true]) {
        for (const [combo, transport] of Object.entries(TRANSPORT_COMBOS)) {
          const exchange = new Exchange(
            exchangeName,
            { ...BASE_OPTIONS, ...transport, publicDataOnly },
            new DefaultStore(),
          );
          liveInstances.push(exchange);
          selection[`${publicDataOnly ? 'public' : 'private'} ${combo}`] = collectAxiosInstances(exchange);
        }
      }

      const rust = Exchange === RustEngineExchange;
      expect(Object.values(selection).every((instances) => rust ? Object.keys(instances).length === 0 : Object.keys(instances).length > 0)).toBe(true);
      expect(selection).toMatchSnapshot();
    },
    ADAPTER_SELECTION_TIMEOUT_MS,
  );
});
