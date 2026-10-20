import type { Exchange } from '../../exchanges/base';
import type { ExchangeName, FetchedLeverage } from '../../types';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { describe, expect, it } from 'vitest';

import { OrderStopTriggerType } from '../../types';
import { UiStateHandler } from '../../webworker/handlers/UiStateHandler';

// Rust venues (Bitrue, BloFin, GRVT, ...) resolve fetchLeverageForSymbol to null when they hold
// no confirmed leverage; a SETTINGS frame may confirm leverage while that fetch is in flight.
const setup = () => {
  const state = { fetchedLeverage: {} as Record<string, FetchedLeverage>, positions: [] };
  const store = {
    get fetchedLeverage() {
      return state.fetchedLeverage;
    },
    get positions() {
      return state.positions;
    },
    update: (changes: Partial<typeof state>) => Object.assign(state, changes),
    updatePositions: () => {},
  };
  let settle!: { resolve: (value: FetchedLeverage | null) => void; reject: (error: unknown) => void };
  const exchange = {
    name: 'bitrue',
    store,
    fetchLeverageForSymbol: () =>
      new Promise<FetchedLeverage | null>((resolve, reject) => {
        settle = { resolve, reject };
      }),
    getActiveTickerSymbols: () => [],
    setActiveTickerSymbols: () => {},
  } as unknown as Exchange;
  const metadata: ExchangeMetadataWorkerType = {
    supportedTPSLTriggerTypes: [],
    privateProxyIps: [],
    defaultTPTriggerType: undefined,
    defaultSLTriggerType: undefined,
    fetchLeverageOnMarginSymbolChange: true,
  };
  const settings = {
    placeOrder: { tpTriggerType: OrderStopTriggerType.MarkPrice, slTriggerType: OrderStopTriggerType.LastPrice },
  };
  const handler = new UiStateHandler('acct', 'bitrue' as ExchangeName, exchange, {} as Worker, settings, metadata);
  handler.handle({ op: 'ui:state', data: { activeMarginSymbol: 'BTCUSDT' } });
  return { state, settle };
};

const confirmed: FetchedLeverage = { buyLeverage: 20, sellLeverage: 20 };
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('UiStateHandler active-margin leverage fetch', () => {
  it('keeps frame-confirmed leverage when the in-flight fetch resolves null', async () => {
    const { state, settle } = setup();
    expect(state.fetchedLeverage.BTCUSDT?.isFetching).toBe(true);

    state.fetchedLeverage = { BTCUSDT: confirmed };
    settle.resolve(null);
    await flush();

    expect(state.fetchedLeverage.BTCUSDT).toEqual(confirmed);
  });

  it('keeps frame-confirmed leverage when the in-flight fetch rejects', async () => {
    const { state, settle } = setup();

    state.fetchedLeverage = { BTCUSDT: confirmed };
    settle.reject(new Error('timeout'));
    await flush();

    expect(state.fetchedLeverage.BTCUSDT).toEqual(confirmed);
  });

  it('clears a placeholder that frame recovery copied while the fetch was in flight', async () => {
    const { state, settle } = setup();

    state.fetchedLeverage = { BTCUSDT: { ...state.fetchedLeverage.BTCUSDT } };
    settle.resolve(null);
    await flush();

    expect(state.fetchedLeverage).not.toHaveProperty('BTCUSDT');
  });

  it('clears its own placeholder when nothing confirmed leverage', async () => {
    const { state, settle } = setup();

    settle.resolve(null);
    await flush();

    expect(state.fetchedLeverage).not.toHaveProperty('BTCUSDT');
  });
});
