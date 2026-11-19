import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { RemoteSigner } from '../../types';

import { createHmac } from 'node:crypto';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { FrameStore } from '../../store/frameStore';
import { OrderSide, OrderType } from '../../types';
import { prepareRecordedHttp } from '../support/managedHttp';

type Kind = 'StopLoss' | 'TakeProfit';
type Scope = 'Full' | 'Partial';
type ProofAxis = 'target' | 'live' | 'history';
type Owner = { ownerId: string; generation: number } | undefined;
type ReadFault = 'none' | 'target' | 'live' | 'history' | 'afterCancel' | 'network' | 'denied' | 'changed';
const cleanup: Array<() => void> = [];
afterEach(() => cleanup.splice(0).forEach((dispose) => dispose()));

async function account(
  kind: Kind,
  scope: Scope = 'Partial',
  localPage = false,
  catalogue?: { entered(): void; resume: Promise<void> },
) {
  const calls: Array<{ role: string; method: string; path: string; query: string; body?: string }> = [];
  let cancelled = false;
  let fault: ReadFault = 'none';
  let afterCancel = () => {};
  let heldProof: { axis: ProofAxis; entered: () => void; resume: Promise<void> } | undefined;
  let heldCancel: { entered: () => void; resume: Promise<void> } | undefined;
  let beforeCancelReady: (() => void) | undefined;
  let owner: Owner;
  const original = {
    category: 'linear',
    symbol: 'BTCUSDT',
    orderId: 'protective-1',
    side: 'Sell',
    orderType: 'Market',
    orderStatus: 'Untriggered',
    stopOrderType: kind,
    tpslMode: scope,
    qty: scope === 'Full' ? '0' : '7',
    cumExecQty: '0',
    cumExecValue: '0',
    avgPrice: '0',
    price: '0',
    leavesQty: scope === 'Full' ? '0' : '7',
    triggerPrice: kind === 'StopLoss' ? '90' : '110',
    triggerBy: 'MarkPrice',
    reduceOnly: true,
    closeOnTrigger: true,
    positionIdx: 0,
    parentOrderLinkId: '',
    orderLinkId: 'standalone-link',
    createdTime: String(Date.now() - 1000),
  };
  const position = {
    category: 'linear',
    symbol: 'BTCUSDT',
    positionIdx: 0,
    side: 'Buy',
    size: '20',
    avgPrice: '100',
    markPrice: '100',
    leverage: '10',
    unrealisedPnl: '0',
    liqPrice: '50',
    updatedTime: String(Date.now()),
  };
  const list = (rows: unknown[]) => ({ category: 'linear', list: rows, nextPageCursor: '' });
  const transport = (role: string): Transport => ({
    prepareHttp: async function (request) {
      const prepared = await prepareRecordedHttp.call(this, request);
      const cancelling = role === 'page' && new URL(request.url).pathname.includes('/order/cancel');
      if (cancelling && heldCancel) {
        const held = heldCancel;
        heldCancel = undefined;
        held.entered();
        await held.resume;
      }
      return {
        ...prepared,
        bind(packet) {
          const bound = prepared.bind(packet);
          return {
            ...bound,
            ready() {
              if (cancelling && beforeCancelReady) {
                const before = beforeCancelReady;
                beforeCancelReady = undefined;
                before();
              }
              return bound.ready();
            },
          };
        },
      };
    },
    now: () => Date.now(),
    connect: (_url, handlers) => {
      queueMicrotask(handlers.onOpen);
      return {
        send: (text) => {
          const message = JSON.parse(text);
          if (message.op === 'auth' || message.op === 'subscribe') {
            queueMicrotask(() =>
              handlers.onMessage(
                JSON.stringify({
                  op: message.op,
                  success: true,
                  ret_msg: '',
                  req_id: message.req_id,
                  conn_id: 'fixture',
                }),
              ),
            );
          }
        },
        close() {},
      };
    },
    request: async ({ method, url, body }) => {
      const parsed = new URL(url);
      const path = parsed.pathname;
      calls.push({ role, method, path, query: parsed.searchParams.toString(), body });
      let result: unknown;
      let retExtInfo: unknown = {};
      switch (path) {
        case '/v5/market/instruments-info':
          if (role === 'page' && catalogue) {
            catalogue.entered();
            await catalogue.resume;
          }
          result = list(
            parsed.searchParams.has('status')
              ? []
              : [
                  {
                    symbol: 'BTCUSDT',
                    status: 'Trading',
                    contractType: 'LinearPerpetual',
                    baseCoin: 'BTC',
                    quoteCoin: 'USDT',
                    settleCoin: 'USDT',
                    priceFilter: { tickSize: '0.1' },
                    lotSizeFilter: { qtyStep: '0.001', minOrderQty: '0.001', maxOrderQty: '120' },
                  },
                ],
          );
          break;
        case '/v5/market/tickers':
          result = list([
            {
              symbol: 'BTCUSDT',
              lastPrice: '100',
              markPrice: '100',
              indexPrice: '100',
              bid1Price: '99.9',
              ask1Price: '100.1',
            },
          ]);
          break;
        case '/v5/account/info':
          result = { marginMode: 'REGULAR_MARGIN', unifiedMarginStatus: 6 };
          break;
        case '/v5/account/wallet-balance':
          result = list([
            {
              accountType: 'UNIFIED',
              totalEquity: '10000',
              totalWalletBalance: '10000',
              totalMarginBalance: '10000',
              totalAvailableBalance: '10000',
              totalInitialMargin: '0',
              totalPerpUPL: '0',
              coin: [],
            },
          ]);
          break;
        case '/v5/account/fee-rate':
          result = list([]);
          break;
        case '/v5/position/list':
          result = list([position]);
          break;
        case '/v5/order/realtime':
          result = list(
            parsed.searchParams.has('orderId')
              ? [{ ...original, orderStatus: cancelled ? 'Cancelled' : 'Untriggered' }]
              : cancelled
                ? []
                : [original],
          );
          break;
        case '/v5/order/history':
          result = list([]);
          break;
        case '/v5/order/cancel-batch':
          cancelled = true;
          afterCancel();
          result = list([{ orderId: original.orderId }]);
          retExtInfo = { list: [{ code: 0, msg: 'OK' }] };
          break;
        case '/v5/order/cancel':
          cancelled = true;
          afterCancel();
          result = { orderId: original.orderId };
          break;
        case '/v5/position/trading-stop':
          result = {};
          break;
        default:
          throw new Error(`Unexpected fixture request ${method} ${path}`);
      }
      const axis =
        path === '/v5/order/history'
          ? 'history'
          : path === '/v5/order/realtime'
            ? parsed.searchParams.has('orderId')
              ? 'target'
              : 'live'
            : undefined;
      if (role === 'page' && axis && (fault === axis || (fault === 'afterCancel' && cancelled))) {
        result = { category: 'linear', list: null, nextPageCursor: '' };
      }
      if (role === 'page' && axis && fault === 'network') throw new Error('Fixture proof transport unavailable');
      if (role === 'page' && axis && fault === 'denied') {
        return {
          status: 200,
          body: new TextEncoder().encode(JSON.stringify({ retCode: 10005, retMsg: 'Permission denied', result: {} })),
        };
      }
      if (role === 'page' && axis === 'target' && fault === 'changed') {
        result = list([{ ...original, triggerPrice: '89', orderStatus: cancelled ? 'Cancelled' : 'Untriggered' }]);
      }
      if (role === 'page' && heldProof && axis === heldProof.axis) {
        const held = heldProof;
        heldProof = undefined;
        held.entered();
        await held.resume;
      }
      return {
        status: 200,
        body: new TextEncoder().encode(JSON.stringify({ retCode: 0, retMsg: 'OK', result, retExtInfo })),
      };
    },
    setTimeout,
    clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  });
  let available = true;
  let refusesProofSigning = false;
  let availabilityChanged: (available: boolean) => void = () => {};
  const signer: RemoteSigner = {
    accountName: 'fixture',
    sign: async (request) => {
      if (!request.wsAuth) throw new Error('Expected a Bybit HMAC signing statement');
      return {
        id: request.id,
        wsAuthSignature: createHmac('sha256', 'fixture-secret').update(request.wsAuth.toSign).digest('hex'),
      };
    },
    isAvailable: () => available,
    onAvailabilityChange: (listener) => {
      availabilityChanged = listener;
      return () => {};
    },
  };
  let transportIndex = 0;
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport(() => transport(transportIndex++ === 0 ? 'source' : 'page'));
  const source = new RustEngineExchange(
    'bybit',
    { key: 'fixture-key', secret: '', remoteSigner: signer },
    new FrameStore(),
  );
  cleanup.push(() => source.dispose());
  await source.start();
  const pageSigner: RemoteSigner = {
    ...signer,
    sign: async (request) =>
      refusesProofSigning && request.wsAuth?.toSign.includes('category=linear&symbol=BTCUSDT')
        ? { id: request.id, error: 'Signing provider is unavailable' }
        : signer.sign(request),
  };
  const page = new RustEngineExchange(
    'bybit',
    {
      key: 'fixture-key',
      ...(localPage ? { secret: 'fixture-secret' } : { secret: '', remoteSigner: pageSigner }),
      workerMirror: true,
      publicDataOnly: false,
    },
    new FrameStore(),
  );
  cleanup.push(() => {
    source.onFrame = undefined;
    page.dispose();
  });
  await page.frameFedReady;
  source.onFrame = (bytes, snapshot) => page.ingestSourceFrame(bytes, snapshot === true);
  const snapshot = source.snapshotFrame();
  if (!snapshot) throw new Error('Missing source snapshot');
  page.ingestSourceFrame(snapshot, true);
  const rpc: string[] = [];
  page.on('callMethod', (event: { method: string; requestId: string }) => {
    rpc.push(event.method);
    page.emitter.emit('callMethod:done', {
      requestId: event.requestId,
      success: false,
      result: { name: 'Error', message: 'verifying', outcome: 'notSent' },
    });
  });
  await vi.waitFor(() => {
    expect(page.store.orders.some((order) => order.id === original.orderId)).toBe(true);
    expect(page.appliedSections.markets && page.store.loaded.markets).toBe(true);
  });
  const order = page.store.orders.find((row) => row.id === original.orderId);
  if (!order) throw new Error('Missing protective fixture order');
  calls.length = 0;
  return {
    page,
    source,
    rpc,
    calls,
    order,
    edit: () => page.updateOrder({ order, update: { price: kind === 'StopLoss' ? 95 : 115 } }),
    fault: (value: ReadFault) => {
      fault = value;
    },
    refuseProofSigning: () => {
      refusesProofSigning = true;
    },
    disableSigner: () => {
      available = false;
      availabilityChanged(false);
    },
    afterCancel: (callback: () => void) => {
      afterCancel = callback;
    },
    bindOwner: (value: Owner = { ownerId: 'A', generation: 1 }) => {
      owner = value;
      page.setWorkerOwner(() => owner);
    },
    changeOwner: (value: Owner) => {
      owner = value;
    },
    pauseCancel: () => {
      const entered = Promise.withResolvers<void>();
      const resumed = Promise.withResolvers<void>();
      heldCancel = { entered: entered.resolve, resume: resumed.promise };
      return { entered: entered.promise, resume: resumed.resolve };
    },
    beforeCancelReady: (callback: () => void) => {
      beforeCancelReady = callback;
    },
    pauseProof: (axis: ProofAxis = 'target') => {
      const entered = Promise.withResolvers<void>();
      const resumed = Promise.withResolvers<void>();
      heldProof = { axis, entered: entered.resolve, resume: resumed.promise };
      return { entered: entered.promise, resume: resumed.resolve };
    },
    changeSourceOrder: async () => {
      const oldPrice = order.price;
      original.triggerPrice = '89';
      await source.refreshAccountOrders(['BTCUSDT']);
      await vi.waitFor(() => {
        expect(page.store.orders.find((row) => row.id === original.orderId)?.price).not.toBe(oldPrice);
      });
    },
    // Public catalogue startup is independent of the private proof/write path.
    pageCalls: () => calls.filter((call) => call.role === 'page' && !call.path.startsWith('/v5/market/')),
    writes: () => calls.filter((call) => call.role === 'page' && call.method === 'POST'),
  };
}

describe('Bybit frame-fed COSIGNED protective edit preflight (real WASM)', () => {
  it('waits for the page catalogue even when source markets and orders are loaded', async () => {
    let entered = () => {};
    let release = () => {};
    const started = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const resume = new Promise<void>((resolve) => {
      release = resolve;
    });
    const fixture = await account('StopLoss', 'Partial', true, { entered, resume });
    await started;
    let settled = false;
    const result = fixture.edit().then(
      () => {
        settled = true;
        return 'applied';
      },
      (error: unknown) => {
        settled = true;
        return error;
      },
    );
    try {
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(fixture.page.store.loaded.markets).toBe(true);
      expect(settled).toBe(false);
      expect(fixture.writes()).toEqual([]);
    } finally {
      release();
    }
    await expect(result).resolves.toBe('applied');
    expect(fixture.writes().map((call) => call.path)).toEqual(['/v5/order/cancel-batch', '/v5/position/trading-stop']);
  }, 15_000);

  for (const localPage of [false, true]) {
    for (const axis of ['target', 'live', 'history'] as const) {
      for (const transition of ['B', 'ABA', 'unavailable', 'unchanged'] as const) {
        it(`fences ${localPage ? 'local' : 'remote'} page ${axis} proof across owner ${transition}`, async () => {
          const fixture = await account('StopLoss', 'Partial', localPage);
          fixture.bindOwner();
          const gate = fixture.pauseProof(axis);
          const result = fixture.edit().catch((error: unknown) => error);
          try {
            await gate.entered;
            if (transition !== 'unchanged') fixture.changeOwner({ ownerId: 'B', generation: 2 });
            if (transition === 'ABA') fixture.changeOwner({ ownerId: 'A', generation: 3 });
            if (transition === 'unavailable') fixture.changeOwner(undefined);
          } finally {
            gate.resume();
          }
          if (transition === 'unchanged') {
            await expect(result).resolves.toEqual([]);
            expect(fixture.writes().map((call) => call.path)).toEqual([
              '/v5/order/cancel-batch',
              '/v5/position/trading-stop',
            ]);
          } else {
            await expect(result).resolves.toMatchObject({ outcome: 'notSent' });
            expect(fixture.writes()).toEqual([]);
          }
        }, 15_000);
      }
    }
    it(`refuses an initially unavailable owner on a ${localPage ? 'local' : 'remote'} page`, async () => {
      const fixture = await account('StopLoss', 'Partial', localPage);
      fixture.bindOwner();
      fixture.changeOwner(undefined);
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'notSent' });
      expect(fixture.writes()).toEqual([]);
    }, 15_000);
    it(`rechecks owner after deferred cancel preparation on a ${localPage ? 'local' : 'remote'} page`, async () => {
      const fixture = await account('StopLoss', 'Partial', localPage);
      fixture.bindOwner();
      const gate = fixture.pauseCancel();
      const result = fixture.edit().catch((error: unknown) => error);
      try {
        await gate.entered;
        fixture.changeOwner({ ownerId: 'B', generation: 2 });
      } finally {
        gate.resume();
      }
      await expect(result).resolves.toMatchObject({ outcome: 'notSent' });
      expect(fixture.writes()).toEqual([]);
    }, 15_000);
    it(`rechecks owner after cancel readiness on a ${localPage ? 'local' : 'remote'} page`, async () => {
      const fixture = await account('StopLoss', 'Partial', localPage);
      fixture.bindOwner();
      fixture.beforeCancelReady(() => fixture.changeOwner({ ownerId: 'B', generation: 2 }));
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'notSent' });
      expect(fixture.writes()).toEqual([]);
    }, 15_000);
    it(`keeps owner loss after cancel Unknown and fenced on a ${localPage ? 'local' : 'remote'} page`, async () => {
      const fixture = await account('StopLoss', 'Partial', localPage);
      fixture.bindOwner();
      fixture.afterCancel(() => fixture.changeOwner({ ownerId: 'B', generation: 2 }));
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'unknown' });
      expect(fixture.writes().map((call) => call.path)).toEqual(['/v5/order/cancel-batch']);
      await expect(
        fixture.page.placeOrder({
          symbol: 'BTCUSDT',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          amount: 1,
          price: 100,
        }),
      ).rejects.toThrow('uncertain');
    }, 15_000);
  }
  for (const kind of ['StopLoss', 'TakeProfit'] as const) {
    it(`checks ${kind} proof before a local page cancels a COSIGNED source's stop`, async () => {
      const fixture = await account(kind, 'Partial', true);
      fixture.fault('target');
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'notSent' });
      expect(fixture.writes()).toEqual([]);
      expect(fixture.rpc).toEqual([]);
    }, 15_000);
    it(`does not gate a healthy local ${kind} edit on an unrelated unavailable source signer`, async () => {
      const fixture = await account(kind, 'Partial', true);
      fixture.disableSigner();
      await expect(fixture.source.readPositions()).rejects.toThrow('Signing provider');
      await expect(fixture.edit()).resolves.toEqual([]);
      expect(fixture.writes().map((call) => call.path)).toEqual([
        '/v5/order/cancel-batch',
        '/v5/position/trading-stop',
      ]);
      expect(fixture.pageCalls().filter((call) => call.method === 'GET')).toHaveLength(6);
      expect(fixture.rpc).toEqual([]);
    }, 15_000);
    for (const axis of ['target', 'live', 'history', 'network', 'denied', 'changed'] as const) {
      it(`refuses ${kind} without cancelling when ${axis} proof cannot be read`, async () => {
        const fixture = await account(kind);
        fixture.fault(axis);
        await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'notSent' });
        expect(fixture.writes()).toEqual([]);
        expect(fixture.rpc).toEqual([]);
        expect(fixture.page.store.orders).toContainEqual(fixture.order);
      }, 15_000);
    }
    it(`refuses ${kind} before cancellation when signing is unavailable`, async () => {
      const fixture = await account(kind);
      fixture.disableSigner();
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'notSent' });
      expect(fixture.writes()).toEqual([]);
    }, 15_000);
    it(`refuses ${kind} when the proof signer refuses despite advertising availability`, async () => {
      const fixture = await account(kind);
      fixture.refuseProofSigning();
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'notSent' });
      expect(fixture.writes()).toEqual([]);
    }, 15_000);
    it(`keeps ${kind} cancel-then-place and post-cancel proof`, async () => {
      const fixture = await account(kind);
      await expect(fixture.edit()).resolves.toEqual([]);
      expect(fixture.writes().map((call) => call.path)).toEqual([
        '/v5/order/cancel-batch',
        '/v5/position/trading-stop',
      ]);
      expect(fixture.writes().map((call) => call.body)).toEqual([
        '{"category":"linear","request":[{"symbol":"BTCUSDT","orderId":"protective-1"}]}',
        kind === 'StopLoss'
          ? '{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Partial","stopLoss":"95","slTriggerBy":"MarkPrice","slSize":"7"}'
          : '{"category":"linear","symbol":"BTCUSDT","positionIdx":0,"tpslMode":"Partial","takeProfit":"115","tpTriggerBy":"MarkPrice","tpSize":"7"}',
      ]);
      const calls = fixture.pageCalls();
      const cancelIndex = calls.findIndex((call) => call.method === 'POST');
      expect(calls.slice(0, cancelIndex).map((call) => call.path)).toEqual([
        '/v5/order/realtime',
        '/v5/order/realtime',
        '/v5/order/history',
      ]);
      expect(calls.slice(cancelIndex + 1, -1).map((call) => call.path)).toEqual([
        '/v5/order/realtime',
        '/v5/order/realtime',
        '/v5/order/history',
      ]);
      expect(fixture.rpc).toEqual([]);
    }, 15_000);
  }
  for (const failure of ['proof', 'signer'] as const) {
    it(`keeps Unknown and fences new placements if ${failure} is lost after cancellation`, async () => {
      const fixture = await account('StopLoss');
      if (failure === 'proof') fixture.fault('afterCancel');
      else fixture.afterCancel(() => fixture.disableSigner());
      await expect(fixture.edit()).rejects.toMatchObject({ outcome: 'unknown' });
      await expect(
        fixture.page.placeOrder({
          symbol: 'BTCUSDT',
          side: OrderSide.Buy,
          type: OrderType.Limit,
          amount: 1,
          price: 100,
        }),
      ).rejects.toThrow('uncertain');
      expect(fixture.writes().map((call) => call.path)).toEqual(['/v5/order/cancel-batch']);
    }, 15_000);
  }
  it('does not cancel after the captured order changes during a proof read', async () => {
    const fixture = await account('StopLoss');
    const gate = fixture.pauseProof();
    const result = fixture.edit().catch((error: unknown) => error);
    try {
      await gate.entered;
      await fixture.changeSourceOrder();
    } finally {
      gate.resume();
    }
    await expect(result).resolves.toMatchObject({ outcome: 'notSent' });
    expect(fixture.writes()).toEqual([]);
  }, 15_000);
  it('cannot release a late cancellation after the page is retired during a proof read', async () => {
    const fixture = await account('StopLoss');
    const gate = fixture.pauseProof();
    const result = fixture.edit().catch((error: unknown) => error);
    await gate.entered;
    fixture.page.dispose();
    gate.resume();
    await result;
    expect(fixture.writes()).toEqual([]);
  }, 15_000);
  it('keeps Full TP/SL in-place without a cancel or proof preflight', async () => {
    const fixture = await account('StopLoss', 'Full');
    await expect(fixture.edit()).resolves.toEqual([]);
    expect(fixture.pageCalls().map((call) => call.path)).toEqual(['/v5/position/trading-stop']);
  }, 15_000);
});
