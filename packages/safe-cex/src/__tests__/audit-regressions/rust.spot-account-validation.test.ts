import { installNativeStartupFrames } from './rust.native-startup.fixtures';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import { afterEach, expect, it, vi } from 'vitest';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { engineCapabilities } from '../fixtures/engineCapabilities';
import { consumerFrame } from '../cutover/consumerFrame';

const exchanges: RustEngineExchange[] = [];
afterEach(() => { exchanges.splice(0).forEach((exchange) => exchange.dispose()); vi.restoreAllMocks(); });
const setup = (reject = true) => {
  const empty = () => '[]';
  const start = vi.fn(() => reject ? '[{"op":"deactivate"}]' : '[]');
  const engine: WasmEngineLike = {
    capabilities: () => engineCapabilities('hyperliquidspot'),
    start, command: empty,
    takeFrame: () => new Uint8Array(consumerFrame({
      loaded: { markets: !reject, tickers: !reject, balance: !reject, orders: !reject, positions: !reject },
    })),
    snapshotFrame: () => new Uint8Array(),
    wsOpen: empty, wsMessage: empty, wsClose: empty, wsRouteExhausted: empty, httpStart: empty,
    httpChunk: empty, httpEnd: empty, httpFailed: empty, timer: empty,
  };
  const construct = vi.fn(function () { return engine; });
  setWasmEngineLoader(async () => ({ WasmEngine: construct as unknown as WasmEngineModule['WasmEngine'] }));
  const store = new DefaultStore();
  installNativeStartupFrames(engine, store);
  // A reused account store must not turn old loaded flags into validation.
  store.update({ loaded: { markets: true, tickers: true, orders: true, positions: true, balance: true } });
  const exchange = new RustEngineExchange('hyperliquidspot', { key: 'wallet', secret: 'invalid' }, store);
  exchange.emitter.on('error', () => {});
  exchanges.push(exchange);
  return { exchange, start, construct };
};

it('checks native spot startup before accepting a reused loaded balance', async () => {
  const { exchange, start } = setup();
  await expect(exchange.validateAccount()).resolves.toMatchObject({ error: expect.stringMatching(/credentials were rejected/) });
  expect(start).toHaveBeenCalledOnce();
});

it('retains a recorded native spot rejection on subsequent account validation', async () => {
  const { exchange, construct } = setup();
  await expect(exchange.start()).rejects.toThrow(/credentials were rejected/);
  await expect(exchange.validateAccount()).resolves.toMatchObject({ error: expect.stringMatching(/credentials were rejected/) });
  expect(construct).toHaveBeenCalledOnce();
});

it('rejects validation of a disposed spot account despite loaded balances', async () => {
  const { exchange, construct } = setup(false);
  exchange.dispose();
  await expect(exchange.validateAccount()).resolves.toMatchObject({ error: expect.stringMatching(/disposed/) });
  expect(construct).not.toHaveBeenCalled();
});

it('accepts loaded balances after successful native spot startup', async () => {
  const { exchange, start } = setup(false);
  await expect(exchange.validateAccount()).resolves.toMatchObject({ error: '' });
  expect(start).toHaveBeenCalledOnce();
});
