import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';
import type { WasmEngineModule } from '../exchanges/rust/rust.exchange';

import { afterEach, expect, it, vi } from 'vitest';

import { exchangeMetadata } from '../exchangeMetadata';
import { loadNodeEngine } from '../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setWasmEngineLoader } from '../exchanges/rust/rust.exchange';
import { DefaultStore } from '../store/store.base';
import { consumerFrame } from './cutover/consumerFrame';
import { engineCapabilities } from './fixtures/engineCapabilities';

const exchanges: RustEngineExchange[] = [];
const opts = { symbol: 'BTCUSDT', interval: '15m' as const, from: 1_700_000_000_000, to: 1_700_000_900_000 };
const candles = [{ timestamp: 1_700_000_000, open: 1, high: 2, low: 1, close: 2, volume: 1 }];

const fixture = (privateAccount = true) => {
  const constructions: Array<Record<string, unknown>> = [];
  const commands: Array<{ op: string }> = [];
  const freed = vi.fn();
  const frame = () =>
    new Uint8Array(
      consumerFrame({
        markets: [],
        tickers: [],
        loaded: { markets: true, tickers: true, balance: true, orders: true, positions: true },
      }),
    );
  const idle = () => '[]';
  const Constructor = Object.assign(
    function (_venue: string, json: string) {
      const config = JSON.parse(json);
      constructions.push(config);
      return {
        construction: () =>
          JSON.stringify({ frameFed: config.frameFed === true, publicDataOnly: config.publicDataOnly === true }),
        capabilities: () => engineCapabilities('bybit'),
        start: idle,
        timer: idle,
        wsOpen: idle,
        wsMessage: idle,
        wsClose: idle,
        wsRouteExhausted: idle,
        httpStart: idle,
        httpChunk: idle,
        httpEnd: idle,
        httpFailed: idle,
        takeFrame: frame,
        snapshotFrame: frame,
        applyFrame: idle,
        free: freed,
        command: (json: string) => {
          const command = JSON.parse(json);
          commands.push(command);
          return JSON.stringify([{ op: 'done', command: command.command, result: JSON.stringify(candles) }]);
        },
      } satisfies WasmEngineLike;
    },
    { frameFedSupported: () => false },
  );
  setWasmEngineLoader(async () => ({ WasmEngine: Constructor as unknown as WasmEngineModule['WasmEngine'] }));
  const exchange = new RustEngineExchange(
    'bybit',
    {
      key: privateAccount ? 'k' : '',
      secret: privateAccount ? 's' : '',
      workerMirror: true,
      publicDataOnly: !privateAccount,
      testnet: true,
      bybitSiteId: 'BRA_BTL',
    },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  const bridge = vi.fn();
  exchange.on('callMethod', bridge);
  return { exchange, constructions, commands, freed, bridge };
};

afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  vi.useRealTimers();
});

it('opts only Bitrue, Lighter and LighterRobinhood into worker candle history', () => {
  expect(
    Object.entries(exchangeMetadata)
      .filter(([, meta]) => meta.fetchOHLCVInWorker)
      .map(([name]) => name)
      .sort(),
  ).toEqual(['bitrue', 'lighter', 'lighter-robinhood']);
});

it.each([true, false])(
  'uses a public page source when no frame-fed engine exists (private=%s)',
  async (privateAccount) => {
    vi.useFakeTimers();
    const { exchange, constructions, commands, bridge, freed } = fixture(privateAccount);
    await exchange.frameFedReady;
    const first = exchange.fetchOHLCV(opts);
    void first.catch(() => {});
    await vi.advanceTimersByTimeAsync(300);
    expect(commands.filter(({ op }) => op === 'fetchCandles')).toHaveLength(1);
    await expect(first).resolves.toEqual(candles);
    await expect(exchange.fetchOHLCV(opts)).resolves.toEqual(candles);
    expect(constructions).toHaveLength(1);
    expect(constructions[0]).toMatchObject({ publicDataOnly: true, testnet: true, bybitSiteId: 'BRA_BTL' });
    expect(JSON.stringify(constructions[0])).not.toContain('primary.signing');
    expect(JSON.stringify(constructions[0])).not.toContain('primary.credential');
    expect(bridge).not.toHaveBeenCalled();
    exchange.dispose();
    expect(freed).toHaveBeenCalledOnce();
  },
);
