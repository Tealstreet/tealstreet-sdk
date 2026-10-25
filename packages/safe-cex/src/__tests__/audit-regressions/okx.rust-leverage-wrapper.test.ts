import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';
import { installNativeStartupFrames } from './rust.native-startup.fixtures';

const running: RustEngineExchange[] = [];

const fixture = (name: ExchangeName) => {
  const commands: Array<Record<string, unknown>> = [];
  const noActions = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities(name),
    start: noActions,
    command(json) {
      const command = JSON.parse(json);
      if (command.op === 'setTickerSymbols') return '[]';
      commands.push(command);
      const result = command.op === 'fetchLeverage' ? { buyLeverage: 25, sellLeverage: 25 } : [];
      return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(result) }]);
    },
    // Startup waits for native readiness flags, not the JS store's loaded state.
    takeFrame: () =>
      new Uint8Array(
        consumerFrame({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } }),
      ),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noActions,
    wsMessage: noActions,
    wsClose: noActions, wsRouteExhausted: noActions,
    httpStart: noActions,
    httpChunk: noActions,
    httpEnd: noActions,
    httpFailed: noActions,
    timer: noActions,
  };
  setWasmEngineLoader(async () => ({
    WasmEngine: vi.fn(function () {
      return engine;
    }) as unknown as WasmEngineModule['WasmEngine'],
  }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange(name, { key: 'k', secret: 's', passphrase: 'p' }, store);
  running.push(exchange);
  return { exchange, store, commands };
};

describe('OKX Rust leverage for flat symbols', () => {
  afterEach(() => {
    for (const exchange of running.splice(0)) exchange.dispose();
    vi.restoreAllMocks();
  });

  // A flat symbol has no position row, so the store held no leverage and the bridge answered null.
  it.each(['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const)('%s asks the engine for leverage-info', async (name) => {
    const { exchange, store, commands } = fixture(name);
    await exchange.start();
    expect(await exchange.fetchLeverageForSymbol('BTCUSDT')).toEqual({ buyLeverage: 25, sellLeverage: 25 });
    expect(store.fetchedLeverage.BTCUSDT).toEqual({ buyLeverage: 25, sellLeverage: 25 });
    expect(commands.at(-1)).toMatchObject({ op: 'fetchLeverage', symbol: 'BTCUSDT' });
  });
});
