import type { WasmEngineModule } from '../../exchanges/rust/rust.exchange';
import type { ExchangeName, ExchangeOptions, Order } from '../../types';

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { betaVenueFamily } from '@tealstreet/safe-cex-wasm/beta-venues';
import { BETA_VENUES } from '../../exchanges/rust/betaVenues';
import { RustEngineExchange, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderStatus, OrderType } from '../../types';

// Read the native capability roster, not a second list of selected test venues.
// Unknown declaration syntax fails closed instead of silently shrinking coverage.
const registry = readFileSync(resolve(__dirname, '../../../../../sdk-rs/registry/src/lib.rs'), 'utf8');
const declaration = registry.match(/pub const FRAME_FED_VENUES:\s*&\[&str\]\s*=\s*&\[([\s\S]*?)\];/)?.[1];
if (declaration === undefined) throw new Error('Native FRAME_FED_VENUES declaration not found');
const literals = declaration.replace(/\/\/[^\n]*/g, '');
if (literals.replace(/"[a-z][a-z0-9-]*"|[\s,]/g, '') !== '') {
  throw new Error('Unrecognized native FRAME_FED_VENUES entries');
}
const frameFedVenues = new Set(Array.from(literals.matchAll(/"([a-z][a-z0-9-]*)"/g), ([, name]) => name));
const sourceVenues = BETA_VENUES.filter((venue) => !frameFedVenues.has(venue));

const modes: ReadonlyArray<{
  label: string;
  options: Pick<ExchangeOptions, 'signingMode'>;
  sender: 'worker' | 'hosted';
}> = [
  { label: 'LOCAL', options: { signingMode: 'local' }, sender: 'worker' },
  { label: 'absent signingMode', options: {}, sender: 'worker' },
  { label: 'COSIGNED', options: { signingMode: 'cosigned' }, sender: 'hosted' },
];
const operations = ['placeOrder', 'updateOrder', 'cancelOrders'] as const;
type Operation = (typeof operations)[number];
const entries = ['direct', 'callMethodInWorker'] as const;
type Entry = (typeof entries)[number];

const order: Order = {
  id: 'fixture-order',
  symbol: 'BTCUSDT',
  type: OrderType.Limit,
  side: OrderSide.Buy,
  price: 100,
  amount: 2,
  filled: 0,
  remaining: 2,
  reduceOnly: false,
  status: OrderStatus.Open,
  lastUpdate: 1,
};
const amendment = { order, update: { price: 101 } };
const cancellations = [order];
const argsFor = (operation: Operation): unknown[] =>
  operation === 'placeOrder' ? [order] : operation === 'updateOrder' ? [amendment] : [cancellations];
const invoke = (exchange: RustEngineExchange, operation: Operation, entry: Entry): Promise<unknown> => {
  if (operation === 'placeOrder')
    return entry === 'direct' ? exchange.placeOrder(order) : exchange.callMethodInWorker('placeOrder', order);
  if (operation === 'updateOrder')
    return entry === 'direct' ? exchange.updateOrder(amendment) : exchange.callMethodInWorker('updateOrder', amendment);
  return entry === 'direct'
    ? exchange.cancelOrders(cancellations)
    : exchange.callMethodInWorker('cancelOrders', cancellations);
};

const exchanges: RustEngineExchange[] = [];
afterEach(() => {
  exchanges.splice(0).forEach((exchange) => exchange.dispose());
  vi.restoreAllMocks();
});

const mirror = (venue: ExchangeName, options: Pick<ExchangeOptions, 'signingMode'>, source: 'worker' | 'hosted') => {
  const pageEngine = vi.fn(() => {
    throw new Error('Non-frame-fed venue must not construct a page engine');
  });
  const frameFedSupported = vi.fn((name: string) => frameFedVenues.has(name));
  const module: WasmEngineModule = {
    WasmEngine: Object.assign(pageEngine, { frameFedSupported }) as unknown as WasmEngineModule['WasmEngine'],
  };
  setWasmEngineLoader(async (name) => {
    // A newly admitted SDK name must also have a real beta-bundle family.
    betaVenueFamily(name);
    return module;
  });
  // Synthetic credentials include KuCoin's required constructor passphrase.
  const exchange = new RustEngineExchange(
    venue,
    {
      key: 'fixture-routing-key',
      secret: '11'.repeat(32),
      passphrase: 'fixture-passphrase',
      workerMirror: true,
      publicDataOnly: false,
      ...options,
    },
    new FrameStore(),
  );
  exchanges.push(exchange);
  exchange.setWorkerOwner(() => ({ ownerId: 'fixture-owner', generation: 1 }));
  const senders = { worker: vi.fn(), hosted: vi.fn() };
  const errors: unknown[] = [];
  exchange.on('error', (error: unknown) => errors.push(error));
  // Record and acknowledge the host-owned sender boundary without network I/O.
  exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
    senders[source]({ method: event.method, args: event.args });
    const result =
      event.method === 'updateOrder'
        ? { orderIds: [source], clientOrderId: 'fixture-replacement' }
        : event.method === 'placeOrder'
          ? [source]
          : null;
    exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result });
  });
  return { exchange, senders, pageEngine, frameFedSupported, errors };
};

// Guard the #3509 class: admitted manual writes must not become unsupported/notSent.
// Every future non-frame-fed beta venue automatically gets the complete matrix.
describe('Every non-frame-fed beta venue keeps a manual writer', () => {
  it('derives a nonempty census from both live rosters', () => {
    expect(frameFedVenues.size).toBeGreaterThan(0);
    expect(sourceVenues.length).toBeGreaterThan(0);
    expect(new Set(sourceVenues).size).toBe(sourceVenues.length);
  });

  for (const venue of sourceVenues) {
    for (const mode of modes) {
      for (const entry of entries) {
        it.each(operations)(
          `${venue} ${mode.label} ${entry} %s reaches exactly one ${mode.sender} sender`,
          async (operation) => {
            const { exchange, senders, pageEngine, frameFedSupported, errors } = mirror(
              venue,
              mode.options,
              mode.sender,
            );
            expect(Object.hasOwn(exchange.options, 'signingMode')).toBe(Object.hasOwn(mode.options, 'signingMode'));
            expect(exchange.options.signingMode).toBe(mode.options.signingMode);
            const expectedArgs = structuredClone(argsFor(operation));
            const result = await invoke(exchange, operation, entry);
            if (operation !== 'cancelOrders') {
              expect(result).toHaveLength(1);
              expect(result).toHaveProperty('0', mode.sender);
              if (operation === 'updateOrder') expect(result).toHaveProperty('clientOrderId', 'fixture-replacement');
            }
            expect(senders[mode.sender].mock.calls).toEqual([[{ method: operation, args: expectedArgs }]]);
            expect(senders[mode.sender === 'worker' ? 'hosted' : 'worker']).not.toHaveBeenCalled();
            expect(frameFedSupported).toHaveBeenCalledExactlyOnceWith(venue);
            expect(pageEngine).not.toHaveBeenCalled();
            expect(errors).toEqual([]);
          },
        );
      }
    }

    for (const entry of entries) {
      it.each(operations)(`${venue} DELEGATED ${entry} %s cannot borrow a LOCAL/COSIGNED sender`, async (operation) => {
        const { exchange, senders, pageEngine } = mirror(venue, { signingMode: 'delegated' }, 'hosted');
        await expect(invoke(exchange, operation, entry)).rejects.toMatchObject({
          kind: 'unsupported',
          outcome: 'notSent',
        });
        expect(senders.worker).not.toHaveBeenCalled();
        expect(senders.hosted).not.toHaveBeenCalled();
        expect(pageEngine).not.toHaveBeenCalled();
      });
    }
  }
});
