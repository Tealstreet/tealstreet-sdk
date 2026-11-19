import type { Exchange } from '../../exchanges/base';
import type { ExchangeName, FetchedLeverage } from '../../types';
import type { ExchangeMetadataWorkerType } from '../../webworker/ExchangeMetadataWorkerType';

import { describe, expect, it, vi } from 'vitest';

import { FrameStore } from '../../store/frameStore';
import { OrderStopTriggerType } from '../../types';
import { UiStateHandler } from '../../webworker/handlers/UiStateHandler';
import { applyReferenceFixture } from '../cutover/referenceFixture';

// Rust venues (Bitrue, BloFin, GRVT, ...) resolve fetchLeverageForSymbol to null when they hold
// no confirmed leverage; a SETTINGS frame may confirm leverage while that fetch is in flight.
const setup = () => {
  const store = new FrameStore();
  let settle!: { resolve: (value: FetchedLeverage | null) => void; reject: (error: unknown) => void };
  const exchange = {
    name: 'bitrue',
    store,
    fetchLeverageForSymbol: vi.fn(
      () =>
        new Promise<FetchedLeverage | null>((resolve, reject) => {
          settle = { resolve, reject };
        }),
    ),
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
  return { state: store, settle, handler, exchange };
};

const confirmed: FetchedLeverage = { buyLeverage: 20, sellLeverage: 20 };
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('UiStateHandler active-margin leverage fetch', () => {
  it('keeps frame-confirmed leverage when the in-flight fetch resolves null', async () => {
    const { state, settle } = setup();
    expect(state.fetchedLeverage).not.toHaveProperty('BTCUSDT');

    applyReferenceFixture(state, { fetchedLeverage: { BTCUSDT: confirmed } });
    settle.resolve(null);
    await flush();

    expect(state.fetchedLeverage.BTCUSDT).toMatchObject(confirmed);
  });

  it('keeps frame-confirmed leverage when the in-flight fetch rejects', async () => {
    const { state, settle } = setup();

    applyReferenceFixture(state, { fetchedLeverage: { BTCUSDT: confirmed } });
    settle.reject(new Error('timeout'));
    await flush();

    expect(state.fetchedLeverage.BTCUSDT).toMatchObject(confirmed);
  });

  it('coalesces a duplicate request without publishing a placeholder', async () => {
    const { state, settle, handler, exchange } = setup();

    handler.handle({ op: 'ui:state', data: { activeMarginSymbol: 'BTCUSDT' } });
    expect(exchange.fetchLeverageForSymbol).toHaveBeenCalledTimes(1);
    settle.resolve(null);
    await flush();

    expect(state.fetchedLeverage).not.toHaveProperty('BTCUSDT');
  });

  it('allows another read after an unconfirmed request settles', async () => {
    const { state, settle, handler, exchange } = setup();

    settle.resolve(null);
    await flush();

    expect(state.fetchedLeverage).not.toHaveProperty('BTCUSDT');
    handler.handle({ op: 'ui:state', data: { activeMarginSymbol: 'BTCUSDT' } });
    expect(exchange.fetchLeverageForSymbol).toHaveBeenCalledTimes(2);
  });
});
