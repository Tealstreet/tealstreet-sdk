import { beforeEach, describe, expect, it, vi } from 'vitest';

import { PositionSide } from '../../types';
import { BybitPrivateWebsocket } from './bybit.ws-private';
import { BybitInversePrivateWebsocket } from './bybitinverse.ws-private';
// `bybitspot.ws-private.ts` exports its class under the SAME name as the linear
// one, so it has to be aliased rather than named.
import { BybitPrivateWebsocket as BybitSpotPrivateWebsocket } from './bybitspot.ws-private';

vi.mock('../../utils/virtualClock', () => ({
  virtualClock: {
    syncBybitClientServerTime: vi.fn(async () => {}),
    getCurrentTimeMs: () => 1_700_000_000_000,
    getCurrentTime: () => 1_700_000_000_000,
  },
}));

vi.mock('@tealstreet/sleep/src/sleep', () => ({ sleep: vi.fn(async () => {}) }));

/**
 * Bybit refuses a private `subscribe` on a socket that has not authenticated, so
 * the auth frame must reach the wire first. `auth` is `async`, and that is only
 * safe because the LOCAL path computes an HMAC with no `await` before its send —
 * an async function body runs synchronously up to its first suspension point.
 *
 * Under the hosted exchange-server the first statement IS an await: the
 * signature comes from a `remoteSigner` round-trip to the paired browser. An
 * un-awaited `this.auth()` therefore yields, `subscribe()` goes out on an
 * unauthenticated socket, and the `order`/`position` topics are never
 * established — the orders table stops updating, so a placed order vanishes
 * when its optimistic entry expires and a cancelled one stays on screen.
 *
 * `b7b6822f95` ("remote signing ... WS auth for all exchanges") fixed the linear
 * socket and missed inverse and spot, which is why bybit USDT worked in hosted
 * mode while BTCUSD did not (user report, 2026-10-01).
 *
 * The sockets are constructed for real rather than through `Object.create`:
 * `sendReconnectSequence`, `auth` and `subscribe` are instance arrow PROPERTIES,
 * so a prototype-only object has none of them and every assertion here would
 * fail on a missing function instead of on ordering.
 */

type Frame = Record<string, any>;

const makeSocket = (Ctor: any, exchangeName: string) => {
  const sent: Frame[] = [];

  // A remote signer that only settles on a later macrotask — the shape of a
  // round-trip to the browser. Resolving synchronously would hide the bug.
  const signer = {
    accountName: 'bear5',
    sign: vi.fn(
      async () =>
        new Promise<{ wsAuthSignature: string }>((resolve) => {
          setTimeout(() => resolve({ wsAuthSignature: 'deadbeef' }), 0);
        }),
    ),
  };

  const parent = {
    name: exchangeName,
    isDisposed: false,
    isDemo: false,
    options: { key: 'k', secret: 's', testnet: false, remoteSigner: signer },
    emitter: { emit: vi.fn() },
    emitAndLogError: vi.fn(),
    fetchAndSetPrivateSnapshot: vi.fn(),
    store: { positions: [], marketsBySymbol: {} },
  };

  const socket = new Ctor(parent);
  socket.sendWithLog = (data: string) => {
    sent.push(JSON.parse(data));
  };

  return { socket, sent, signer };
};

const SOCKETS = [
  { label: 'bybit (linear)', Ctor: BybitPrivateWebsocket, name: 'bybit' },
  { label: 'bybitinverse', Ctor: BybitInversePrivateWebsocket, name: 'bybitinverse' },
  { label: 'bybitspot', Ctor: BybitSpotPrivateWebsocket, name: 'bybitspot' },
] as const;

describe('bybit private sockets authenticate before they subscribe', () => {
  it.each(SOCKETS)('$label sends auth before subscribe under a remote signer', async ({ Ctor, name }) => {
    const { socket, sent, signer } = makeSocket(Ctor, name);

    await socket.sendReconnectSequence();

    expect(signer.sign).toHaveBeenCalledTimes(1);

    const authIndex = sent.findIndex((frame) => frame.op === 'auth');
    const subscribeIndex = sent.findIndex((frame) => frame.op === 'subscribe');

    expect(authIndex).toBeGreaterThan(-1);
    expect(subscribeIndex).toBeGreaterThan(-1);
    expect(authIndex).toBeLessThan(subscribeIndex);
  });

  it.each(SOCKETS)('$label carries the signature the remote signer returned', async ({ Ctor, name }) => {
    const { socket, sent } = makeSocket(Ctor, name);

    await socket.sendReconnectSequence();

    const auth = sent.find((frame) => frame.op === 'auth');
    // The third arg is the signature. A bare ordering assertion would still pass
    // if the frame went out empty, which is the other way this can break.
    expect(auth?.args?.[2]).toBe('deadbeef');
  });
});

/**
 * `updatePositions` rewrites entries that are already in the store and returns
 * early when it is empty, so the inverse socket's position topic could only ever
 * refresh a position the REST poll had already discovered — a position opened
 * while the tab was up stayed invisible until the next poll.
 *
 * It also has to stay scoped: bybit's v5 private stream is ONE socket per
 * account whose `position` topic carries every category, while this exchange's
 * `fetchPositions` asks for `category: 'inverse'` only.
 */
describe('the inverse position topic adds positions, scoped to its own markets', () => {
  let socket: any;
  let store: any;

  beforeEach(() => {
    store = {
      positions: [] as any[],
      marketsBySymbol: { BTCUSD: { symbol: 'BTCUSD', isInverse: true } },
      addOrUpdatePositions: vi.fn((positions: any[]) => store.positions.push(...positions)),
      updatePositions: vi.fn(),
    };

    socket = new BybitInversePrivateWebsocket({
      store,
      isDisposed: false,
      mapPosition: (p: Record<string, any>) => ({
        id: `${p.symbol}:long`,
        symbol: p.symbol,
        side: PositionSide.Long,
        contracts: Number(p.size),
      }),
      syncBalanceUpnlFromPositions: vi.fn(),
      lastWebsocketPositionsUpdate: 0,
    } as any);
  });

  it('adds a position that is not in the store yet', () => {
    socket.handlePositionTopic([{ symbol: 'BTCUSD', size: '100' }]);

    expect(store.addOrUpdatePositions).toHaveBeenCalledTimes(1);
    expect(store.positions).toHaveLength(1);
    expect(store.positions[0]).toMatchObject({ symbol: 'BTCUSD', contracts: 100 });
  });

  it('syncs the unrealized pnl into the balance', () => {
    socket.handlePositionTopic([{ symbol: 'BTCUSD', size: '100' }]);

    expect(socket.parent.syncBalanceUpnlFromPositions).toHaveBeenCalledTimes(1);
  });

  it('ignores a frame for a market this exchange does not serve', () => {
    socket.handlePositionTopic([{ symbol: 'DOGEUSDT', size: '500' }]);

    expect(store.addOrUpdatePositions).not.toHaveBeenCalled();
    expect(store.positions).toHaveLength(0);
  });

  it('keeps the inverse rows out of a mixed-category frame', () => {
    socket.handlePositionTopic([
      { symbol: 'DOGEUSDT', size: '500' },
      { symbol: 'BTCUSD', size: '100' },
    ]);

    expect(store.positions.map((p: any) => p.symbol)).toEqual(['BTCUSD']);
  });
});
