import { ApexOmniPrivateWebsocket } from '../reference/apexomni/apexomni.ws-private';
import { virtualClock } from '../../utils/virtualClock';

/**
 * Apex's private channels are established with `op: 'login'` followed by
 * `op: 'subscribe'`, and `onOpen` issues both back to back. `auth` is `async`,
 * which is survivable on the LOCAL path only because its HMAC branch reaches
 * `sendWithLog` with no `await` in front of it — so an un-awaited call still put
 * the login on the wire first. Under the hosted exchange-server the first
 * statement IS an await, because the signature is a round trip to the paired
 * browser, so `subscribe()` went out before the login and the private channels
 * were never established. Same defect class as bybit inverse/spot; see
 * `bybit.private-ws-auth-ordering.test.ts`.
 *
 * `d1184f3d8e` is the reason this looks risky and is not. It reverted three
 * bundled things after the upstream proxy began closing the socket with code
 * 1005: a `topics` field in the auth payload, subscribe gated on the server's
 * login ACK behind a 5s watchdog, and a trimmed channel list. Awaiting our own
 * signer is none of those — the local path still logs in before subscribing, one
 * microtask later, in the same macrotask. These cases pin that: the local-path
 * case asserts login precedes subscribe with the full three-channel args list
 * intact, so restoring the ack-gating or trimming the channels both fail here.
 *
 * On a failed login the socket sends NOTHING and reconnects. `RemoteSigner.sign`
 * throws rather than resolving `{ error }`, and subscribing anyway is worse than
 * silence: `base.ws.ts trackLastReceived` sets `isInitialized` on the first
 * inbound frame, so any frame makes a socket holding zero channels report healthy
 * indefinitely.
 */

const NOW_MS = 1_750_000_000_000;
const APEX_CHANNELS = ['ws_zk_accounts_v3', 'ws_user_v1', 'ws_notify_v1'];

type Send = ReturnType<typeof vi.fn>;

const makeParent = (overrides: Record<string, unknown> = {}) => ({
  isDisposed: false,
  name: 'apexomni',
  options: { key: 'test-key', secret: 'test-secret', passphrase: 'test-pass', testnet: false },
  emitter: { emit: vi.fn() },
  store: { update: vi.fn() },
  log: vi.fn(),
  emitAndLogError: vi.fn(),
  fetchAndSetPrivateSnapshot: vi.fn(),
  ...overrides,
});

const makeSigner = (mode: 'ok' | 'resolves-error' | 'throws') => ({
  accountName: 'fixture-account',
  sign: vi.fn(
    () =>
      new Promise<{ wsAuthSignature?: string; error?: string }>((resolve, reject) => {
        setTimeout(() => {
          if (mode === 'throws') reject(new Error('Remote signer unavailable — WebSocket disconnected (gone <5s)'));
          else if (mode === 'resolves-error') resolve({ error: 'browser closed mid-signature' });
          else resolve({ wsAuthSignature: 'apex-signature' });
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

const open = async (socket: { onOpen: () => void | Promise<void> }) => {
  const opened = socket.onOpen();
  await vi.advanceTimersByTimeAsync(100);
  await opened;
};

describe('apexomni private socket logs in before it subscribes', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
    (virtualClock as any).server = null;
    (virtualClock as any).client = null;
    (virtualClock as any).offsetMs = 0;
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  test('sends login before subscribe under a remote signer', async () => {
    const signer = makeSigner('ok');
    const socket = new ApexOmniPrivateWebsocket(
      makeParent({ options: { key: 'test-key', secret: '', passphrase: 'test-pass', remoteSigner: signer } }) as any,
    );
    const send = attachOpenSocket(socket);

    await open(socket);

    expect(signer.sign).toHaveBeenCalledTimes(1);
    const frames = sentJson(send);
    const loginIndex = frames.findIndex((f) => f.op === 'login');
    const subscribeIndex = frames.findIndex((f) => f.op === 'subscribe');

    expect(loginIndex).toBeGreaterThan(-1);
    expect(subscribeIndex).toBeGreaterThan(-1);
    expect(loginIndex).toBeLessThan(subscribeIndex);
    expect(JSON.parse(frames[loginIndex].args[0]).signature).toBe('apex-signature');
  });

  // The direct path, which is what `d1184f3d8e` was protecting. Login still
  // precedes subscribe, and the three-channel list is intact.
  test('sends login before subscribe on the local HMAC path, keeping all three channels', async () => {
    const socket = new ApexOmniPrivateWebsocket(makeParent() as any);
    const send = attachOpenSocket(socket);

    await open(socket);

    const frames = sentJson(send);
    expect(frames.findIndex((f) => f.op === 'login')).toBeLessThan(frames.findIndex((f) => f.op === 'subscribe'));
    expect(frames.filter((f) => f.op === 'subscribe')).toEqual([{ op: 'subscribe', args: APEX_CHANNELS }]);
  });

  // No `topics` field — its presence correlated with the 1005 server closes.
  test('never puts a topics field in the login payload', async () => {
    const socket = new ApexOmniPrivateWebsocket(makeParent() as any);
    const send = attachOpenSocket(socket);

    await open(socket);

    const login = sentJson(send).find((f) => f.op === 'login');
    expect(login).toBeDefined();
    expect(login).not.toHaveProperty('topics');
    expect(JSON.parse(login.args[0])).not.toHaveProperty('topics');
  });

  test.each(['resolves-error', 'throws'] as const)('sends NOTHING and reconnects when the signer %s', async (mode) => {
    const parent = makeParent({
      options: { key: 'test-key', secret: '', passphrase: 'test-pass', remoteSigner: makeSigner(mode) },
    });
    const socket = new ApexOmniPrivateWebsocket(parent as any);
    const send = attachOpenSocket(socket);
    const reconnect = vi.fn();
    (socket as any).reconnect = reconnect;

    // An escaping rejection would be unhandled — nothing awaits `onOpen`.
    await expect(open(socket)).resolves.toBeUndefined();

    // A subscribe on an unlogged-in socket would be refused, and either frame makes
    // `trackLastReceived` set `isInitialized` and keep `lastReceived` fresh — so a
    // socket holding zero channels would report healthy forever.
    expect(send).not.toHaveBeenCalled();
    expect(reconnect).toHaveBeenCalledTimes(1);
    // Never silent, and the CAUSE must survive: a second `emitAndLogError` argument
    // REPLACES the message for an Error input, and the literal "auth failed" is
    // matched by errorPatterns' `auth_invalid`, which would blame the user's key.
    expect(parent.emitAndLogError).toHaveBeenCalledTimes(1);
    const reported = String((parent.emitAndLogError as any).mock.calls[0][0]);
    expect((parent.emitAndLogError as any).mock.calls[0][1]).toBeUndefined();
    expect(reported.toLowerCase()).not.toContain('auth failed');
  });

  test('never asks the signer once the exchange is disposed', async () => {
    const signer = makeSigner('ok');
    const socket = new ApexOmniPrivateWebsocket(
      makeParent({
        isDisposed: true,
        options: { key: 'test-key', secret: '', passphrase: 'test-pass', remoteSigner: signer },
      }) as any,
    );
    const send = attachOpenSocket(socket);

    await open(socket);

    expect(signer.sign).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
  });
});
