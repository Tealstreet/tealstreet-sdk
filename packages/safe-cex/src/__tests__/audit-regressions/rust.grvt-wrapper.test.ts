vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';

import { expect, it, vi } from 'vitest';

import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { directTransport } from '../../transport/direct';
import { tealstreetTransport } from '../../transport/tealstreet/tealstreetTransport';
import { adoptEngineCapabilities, engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

it.each([true, false])('GRVT SDK uses the transport-selected authentication protocol when proxied=%s', async (proxied) => {
  const noop = () => '[]';
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('grvt'),
    start: noop,
    command: (json) => {
      const command = JSON.parse(json);
      return JSON.stringify([{ op: 'done', command: command.command, result: '[]' }]);
    },
    takeFrame: () => new Uint8Array(consumerFrame({
      loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
    })),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: noop,
    wsMessage: noop,
    wsClose: noop, wsRouteExhausted: noop,
    httpStart: noop,
    httpChunk: noop,
    httpEnd: noop,
    httpFailed: noop,
    timer: noop,
  };
  const construct = vi.fn(function (_venue: string, _options: string) { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  store.update({ loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true } });
  const exchange = new RustEngineExchange('grvt', {
    key: 'fixture-key', secret: 'fixture-secret', grvtTradingAccountId: '777',
    transport: proxied ? tealstreetTransport : directTransport,
  }, store);
  try {
    await exchange.start();
    expect(construct.mock.calls[0][0]).toBe('grvt');
    expect(JSON.parse(construct.mock.calls[0][1])).toMatchObject({
      grvtSessionProxy: proxied, grvtTradingAccountId: '777', publicDataOnly: false,
    });
  } finally {
    exchange.dispose();
  }
});

it.each(['grvt', 'woo'] as const)('%s cancel-all reaches the engine even with an empty local order snapshot', async (name) => {
  const exchange = new RustEngineExchange(name, { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  const command = vi.fn().mockResolvedValue([]);
  Object.assign(exchange, { host: { command: (input: unknown) => command(input), dispose: vi.fn() } });
  adoptEngineCapabilities(exchange);
  try {
    await exchange.cancelAllOrders();
    expect(command).toHaveBeenCalledExactlyOnceWith({ op: 'cancelSymbolOrders', symbol: '' });
  } finally {
    exchange.dispose();
  }
});

it('GRVT global cancellation does not inherit WOO journal position folding', async () => {
  const exchange = new RustEngineExchange('grvt', { key: '', secret: '', publicDataOnly: true }, new DefaultStore());
  adoptEngineCapabilities(exchange);
  const orders = vi.spyOn(exchange, 'fetchHistoricalOrders').mockResolvedValue([]);
  try {
    await expect(exchange.fetchHistoricalPositions({ account: 'fixture' })).rejects.toThrow('no journal records path declared');
    expect(orders).not.toHaveBeenCalled();
  } finally {
    exchange.dispose();
  }
});
