import { OKXBusinessWebsocket } from '../reference/okx/okx.ws-business';
import { OKXPrivateWebsocket } from '../reference/okx/okx.ws-private';

/**
 * OKX's private and business sockets used to call `this.auth()` without awaiting
 * it and ping straight away. Under the hosted exchange-server the signature is a
 * round trip to the browser and `remoteSigner.sign` THROWS when that link is down,
 * so a failed sign was an unhandled rejection, no login ever went out, and the
 * ping kept a socket holding zero topics looking alive. Now a failed sign sends
 * nothing and reconnects, mirroring bybit (`bybit.private-ws-auth-ordering.test.ts`).
 */

const SOCKETS = [
  { label: 'okx private', Ctor: OKXPrivateWebsocket },
  { label: 'okx business', Ctor: OKXBusinessWebsocket },
] as const;

const makeParent = (options: Record<string, unknown>) => ({
  isDisposed: false,
  isDemo: false,
  name: 'okx',
  options: { key: 'k', passphrase: 'p', testnet: false, ...options },
  emitter: { emit: vi.fn() },
  store: { update: vi.fn() },
  log: vi.fn(),
  emitAndLogError: vi.fn(),
  fetchAndSetPrivateSnapshot: vi.fn(),
  getInstrumentTypes: () => ['SWAP'],
});

const makeSigner = (mode: 'ok' | 'resolves-error' | 'throws') => ({
  accountName: 'acct',
  sign: vi.fn(
    () =>
      new Promise<{ wsAuthSignature?: string; error?: string }>((resolve, reject) => {
        setTimeout(() => {
          if (mode === 'throws') reject(new Error('Remote signer unavailable — WebSocket disconnected (gone <5s)'));
          else if (mode === 'resolves-error') resolve({ error: 'browser closed mid-signature' });
          else resolve({ wsAuthSignature: 'c2ln' });
        }, 5);
      }),
  ),
});

const open = async (Ctor: (typeof SOCKETS)[number]['Ctor'], options: Record<string, unknown>) => {
  const parent = makeParent(options);
  const socket = new Ctor(parent as any);
  const send = vi.fn();
  (socket as any).ws = { readyState: WebSocket.OPEN, send };
  const reconnect = vi.spyOn(socket as any, 'reconnect').mockResolvedValue(undefined);
  const opened = socket.onOpen();
  await vi.advanceTimersByTimeAsync(50);
  await opened;
  return { parent, frames: send.mock.calls.map(([payload]) => String(payload)), reconnect };
};

describe('okx private sockets on a failed remote signature', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe.each(SOCKETS)('$label', ({ Ctor }) => {
    test.each(['throws', 'resolves-error'] as const)(
      'sends nothing and reconnects when the signer %s',
      async (mode) => {
        const { parent, frames, reconnect } = await open(Ctor, { secret: '', remoteSigner: makeSigner(mode) });

        expect(frames).toEqual([]);
        expect(reconnect).toHaveBeenCalledTimes(1);
        const reported = parent.emitAndLogError.mock.calls.map(([e]) => String(e instanceof Error ? e.message : e));
        expect(reported.some((m) => /auth failed/i.test(m))).toBe(false);
      },
    );

    test('logs in, then pings, when the signer answers', async () => {
      const { frames, reconnect } = await open(Ctor, { secret: '', remoteSigner: makeSigner('ok') });

      expect(JSON.parse(frames[0])).toMatchObject({ op: 'login', args: [{ apiKey: 'k', sign: 'c2ln' }] });
      expect(frames[1]).toBe('ping');
      expect(reconnect).not.toHaveBeenCalled();
    });

    test('logs in, then pings, on the local HMAC path', async () => {
      const { frames, reconnect } = await open(Ctor, { secret: 's' });

      expect(JSON.parse(frames[0])).toMatchObject({ op: 'login' });
      expect(frames[1]).toBe('ping');
      expect(reconnect).not.toHaveBeenCalled();
    });
  });
});
