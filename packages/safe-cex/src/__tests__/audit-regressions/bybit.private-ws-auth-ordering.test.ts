import { BybitPrivateWebsocket } from '../reference/bybit/bybit.ws-private';
import { BybitInversePrivateWebsocket } from '../reference/bybit/bybitinverse.ws-private';
// `bybitspot.ws-private.ts` exports its class under the SAME name as the linear
// one, so it has to be aliased rather than named.
import { BybitPrivateWebsocket as BybitSpotPrivateWebsocket } from '../reference/bybit/bybitspot.ws-private';
import { virtualClock } from '../../utils/virtualClock';

/**
 * Bybit refuses a private `subscribe` on a socket that has not authenticated.
 * Captured live against `wss://stream.bybit.com/v5/private`, 2026-10-01:
 *
 *   -> {"op":"subscribe","args":["order","position"]}            (no auth first)
 *   <- {"success":false,"ret_msg":"Request not authorized","op":"subscribe"}
 *   -> {"op":"auth",...} <- success:true  -> subscribe <- success:true
 *
 * `auth` is `async`, and that is survivable on the LOCAL path only because it
 * computes an HMAC with no `await` before its send — an async body runs
 * synchronously to its first suspension point. Under the hosted exchange-server
 * the first statement IS an await, because the signature is a round trip to the
 * paired browser, so an un-awaited `this.auth()` lets `subscribe()` reach the
 * wire unauthenticated and the `order`/`position` topics are never established.
 * The orders table then stops updating while REST polling keeps tickers,
 * positions and balance looking healthy.
 *
 * `b7b6822f95` added the `await` to the LINEAR socket and missed inverse and
 * spot, which is why hosted bybit USDT worked and hosted BTCUSD did not.
 *
 * The failure paths matter as much as the ordering, and the first attempt at them
 * was wrong in the opposite direction. `RemoteSigner.sign` THROWS — signer
 * unavailable, the 15s sign timeout, a failed send, `markUnavailable` rejecting
 * pending — rather than resolving `{ error }`, which is the only case `auth`
 * itself handles. On that failure the socket must send NOTHING and reconnect:
 * subscribing anyway looks like recovery and is not, because
 * `base.ws.ts trackLastReceived` sets `isInitialized` on the FIRST inbound frame
 * and bybit answers a ping on an unauthenticated `/v5/private` socket (measured
 * 2026-10-01, 7 pings 7 pongs), so `lastReceived` keeps refreshing, the 20s
 * staleness threshold never trips, and a socket holding zero topics reports
 * healthy forever. The reported error must also never read "auth failed" —
 * `health/errorPatterns.ts` matches that literal as `auth_invalid`, blaming the
 * user's API key and suppressing account reloads for five minutes.
 *
 * Frames are asserted through the REAL `sendWithLog` -> `send()` path against a
 * socket at `readyState OPEN`, matching `private-websocket.auth-resubscribe.test.ts`
 * — stubbing `sendWithLog` on the instance would skip `send()`'s own
 * readyState/disposal handling.
 */

const NOW_MS = 1_750_000_000_000;
/** HMAC-SHA256('test-secret', `GET/realtime${NOW_MS + 10_000 + 25}`), hex. */
const EXPECTED_SIGNATURE = 'f07c78eb62a25f9ffe8bb669cf188e029eff34b59d3f98bc505e66d8c180ed37';
const EXPECTED_EXPIRES = '1750000010025';

type Send = ReturnType<typeof vi.fn>;

const SOCKETS = [
  { label: 'bybit (linear)', Ctor: BybitPrivateWebsocket, topics: ['order', 'position'] },
  { label: 'bybitinverse', Ctor: BybitInversePrivateWebsocket, topics: ['order', 'position'] },
  // Spot asks for `order` only — it has no positions (`fetchPositions` returns `[]`
  // and nothing writes the store), so the position topic fed a dead handler.
  { label: 'bybitspot', Ctor: BybitSpotPrivateWebsocket, topics: ['order'] },
] as const;

const makeParent = (overrides: Record<string, unknown> = {}) => ({
  isDisposed: false,
  isDemo: false,
  name: 'fixture-bybit',
  options: { key: 'test-key', secret: 'test-secret', testnet: false },
  emitter: { emit: vi.fn() },
  store: { update: vi.fn() },
  log: vi.fn(),
  emitAndLogError: vi.fn(),
  fetchAndSetPrivateSnapshot: vi.fn(),
  ...overrides,
});

/** A signer that settles only on a later macrotask — the shape of a round trip to
 *  the browser. `mode` picks which of the three real outcomes it models. */
const makeSigner = (mode: 'ok' | 'resolves-error' | 'throws') => ({
  accountName: 'fixture-account',
  sign: vi.fn(
    (_request: Record<string, any>) =>
      new Promise<{ wsAuthSignature?: string; error?: string }>((resolve, reject) => {
        setTimeout(() => {
          if (mode === 'throws') reject(new Error('Remote signer unavailable — WebSocket disconnected (gone <5s)'));
          else if (mode === 'resolves-error') resolve({ error: 'browser closed mid-signature' });
          else resolve({ wsAuthSignature: 'deadbeef' });
        }, 5);
      }),
  ),
});

const attachOpenSocket = (socket: { ws?: unknown }): Send => {
  const send = vi.fn();
  socket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
  return send;
};

const sentJson = (send: Send) => send.mock.calls.map(([payload]) => JSON.parse(payload));

const run = async (socket: { sendReconnectSequence: (syncTime?: boolean) => Promise<void> }) => {
  const sequence = socket.sendReconnectSequence();
  await vi.advanceTimersByTimeAsync(100);
  await sequence;
};

describe('bybit private sockets authenticate before they subscribe', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
    (virtualClock as any).server = null;
    (virtualClock as any).client = null;
    (virtualClock as any).offsetMs = 0;
    vi.spyOn(virtualClock, 'syncBybitClientServerTime').mockResolvedValue();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe.each(SOCKETS)('$label', ({ Ctor, topics }) => {
    test('sends auth before subscribe under a remote signer', async () => {
      const signer = makeSigner('ok');
      const socket = new Ctor(makeParent({ options: { key: 'test-key', secret: '', remoteSigner: signer } }) as any);
      const send = attachOpenSocket(socket);

      await run(socket);

      expect(signer.sign).toHaveBeenCalledTimes(1);
      // Signing the wrong string, in the wrong format, or as the wrong request type
      // reproduces the original symptom exactly — the venue refuses and orders stop.
      expect(signer.sign.mock.calls[0][0]).toMatchObject({
        type: 'ws-auth',
        wsAuth: { toSign: `GET/realtime${EXPECTED_EXPIRES}`, outputFormat: 'hex' },
      });
      const frames = sentJson(send);
      const authIndex = frames.findIndex((f) => f.op === 'auth');
      const subscribeIndex = frames.findIndex((f) => f.op === 'subscribe');

      expect(authIndex).toBeGreaterThan(-1);
      expect(subscribeIndex).toBeGreaterThan(-1);
      expect(authIndex).toBeLessThan(subscribeIndex);
      // A bare ordering assertion passes on an empty auth frame too.
      expect(frames[authIndex].args[1]).toBe(EXPECTED_EXPIRES);
      expect(frames[authIndex].args[2]).toBe('deadbeef');
    });

    // The hosted secret is `''`, so the local branch is only reachable when no
    // signer is present — and it is every non-hosted user's path.
    test('sends auth before subscribe on the local HMAC path', async () => {
      const socket = new Ctor(makeParent() as any);
      const send = attachOpenSocket(socket);

      await run(socket);

      const frames = sentJson(send);
      expect(frames.findIndex((f) => f.op === 'auth')).toBeLessThan(frames.findIndex((f) => f.op === 'subscribe'));
      expect(frames.find((f) => f.op === 'auth')).toEqual({
        op: 'auth',
        args: ['test-key', EXPECTED_EXPIRES, EXPECTED_SIGNATURE],
      });
    });

    // Dropping a topic from this list is the reported bug by another route, and
    // an ordering-only assertion cannot see it.
    test('subscribes to exactly the topics this variant has data for', async () => {
      const socket = new Ctor(makeParent() as any);
      const send = attachOpenSocket(socket);

      await run(socket);

      expect(sentJson(send).filter((f) => f.op === 'subscribe')).toEqual([{ op: 'subscribe', args: [...topics] }]);
    });

    test.each(['resolves-error', 'throws'] as const)(
      'sends NOTHING and reconnects when the signer %s',
      async (mode) => {
        const parent = makeParent({ options: { key: 'test-key', secret: '', remoteSigner: makeSigner(mode) } });
        const socket = new Ctor(parent as any);
        const send = attachOpenSocket(socket);
        const reconnect = vi.fn();
        (socket as any).reconnect = reconnect;

        // An escaping rejection would be unhandled — `onOpen` does not await this.
        await expect(run(socket)).resolves.toBeUndefined();

        // Bybit refuses a private subscribe on an unauthenticated socket AND still
        // pongs a ping on one (measured 7/7), so sending either would set
        // `isInitialized` and keep `lastReceived` fresh — reporting a socket with
        // zero topics as healthy forever, which is the original bug, not a recovery.
        expect(send).not.toHaveBeenCalled();
        expect(reconnect).toHaveBeenCalledTimes(1);
        // Never silent, and the CAUSE must survive: `emitAndLogError`'s second
        // argument REPLACES the message for an Error input, and the literal
        // "auth failed" is matched by errorPatterns' `auth_invalid`, which would
        // blame the user's API key and suppress reloads for five minutes.
        expect(parent.emitAndLogError).toHaveBeenCalledTimes(1);
        const reported = String((parent.emitAndLogError as any).mock.calls[0][0]);
        expect((parent.emitAndLogError as any).mock.calls[0][1]).toBeUndefined();
        expect(reported.toLowerCase()).not.toContain('auth failed');
        expect(reported).toMatch(mode === 'throws' ? /Remote signer unavailable/ : /signing unavailable/);
      },
    );

    // Nothing else proves the sequence is wired to the socket opening.
    test('runs the sequence when the socket opens', async () => {
      const socket = new Ctor(makeParent() as any);
      const send = attachOpenSocket(socket);

      socket.onOpen();
      await vi.advanceTimersByTimeAsync(100);

      const frames = sentJson(send);
      expect(frames.findIndex((f) => f.op === 'auth')).toBeLessThan(frames.findIndex((f) => f.op === 'subscribe'));
    });

    // Asserting only "no frames" is vacuous — `send()` has its own disposal
    // guard, so it holds with the entry check removed. The load-bearing claim is
    // that a disposed account never SIGNS again: the exchange-server incident
    // that rule comes from was a signer flood, not stray frames.
    test('never asks the signer once the exchange is disposed', async () => {
      const signer = makeSigner('ok');
      const socket = new Ctor(
        makeParent({ isDisposed: true, options: { key: 'test-key', secret: '', remoteSigner: signer } }) as any,
      );
      const send = attachOpenSocket(socket);

      await run(socket);

      expect(signer.sign).not.toHaveBeenCalled();
      expect(send).not.toHaveBeenCalled();
    });
  });
});
