import type { BybitExchange } from '../reference/bybit/bybit.exchange';
import type { OKXExchange } from '../reference/okx/okx.exchange';
import type { BitgetExchange } from '../reference/bitget/bitget.exchange';

import { BitgetPrivateWebsocket } from '../reference/bitget/bitget.ws-private';
import { BybitPrivateWebsocket } from '../reference/bybit/bybit.ws-private';
import { OKXPrivateWebsocket } from '../reference/okx/okx.ws-private';
import { virtualClock } from '../../utils/virtualClock';

const NOW_MS = 1_750_000_000_000;

const commonParent = () => ({
  isDisposed: false,
  name: 'fixture-exchange',
  options: {
    key: 'test-key',
    secret: 'test-secret',
    passphrase: 'test-passphrase',
  },
  emitter: { emit: vi.fn() },
  store: { update: vi.fn() },
  log: vi.fn(),
  emitAndLogError: vi.fn(),
  fetchAndSetPrivateSnapshot: vi.fn(),
});

const attachOpenSocket = (socket: { ws?: unknown }) => {
  const send = vi.fn();
  socket.ws = { readyState: WebSocket.OPEN, send } as unknown as WebSocket;
  return send;
};

const sentJson = (send: ReturnType<typeof vi.fn>) => send.mock.calls.map(([payload]) => JSON.parse(payload));

describe('private WebSocket auth and reconnect subscriptions', () => {
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

  test('Bybit signs GET/realtime and replays order and position subscriptions', async () => {
    const parent = {
      ...commonParent(),
      isDemo: false,
    } as unknown as BybitExchange;
    const socket = new BybitPrivateWebsocket(parent);
    const send = attachOpenSocket(socket);

    const firstReconnect = socket.sendReconnectSequence();
    await vi.advanceTimersByTimeAsync(25);
    await firstReconnect;

    expect(sentJson(send)).toContainEqual({
      op: 'auth',
      args: [
        'test-key',
        '1750000010025',
        'f07c78eb62a25f9ffe8bb669cf188e029eff34b59d3f98bc505e66d8c180ed37',
      ],
    });
    expect(sentJson(send)).toContainEqual({ op: 'subscribe', args: ['order', 'position'] });

    const secondReconnect = socket.sendReconnectSequence();
    await vi.advanceTimersByTimeAsync(25);
    await secondReconnect;

    expect(sentJson(send).filter((payload) => payload.op === 'subscribe')).toEqual([
      { op: 'subscribe', args: ['order', 'position'] },
      { op: 'subscribe', args: ['order', 'position'] },
    ]);
  });

  test('OKX signs the private login and resubscribes after every successful login', async () => {
    const parent = {
      ...commonParent(),
      name: 'okx',
      region: 'global',
      getInstrumentTypes: () => ['SWAP'],
    } as unknown as OKXExchange;
    const socket = new OKXPrivateWebsocket(parent);
    const send = attachOpenSocket(socket);

    await socket.auth();

    expect(sentJson(send)).toContainEqual({
      op: 'login',
      args: [
        {
          apiKey: 'test-key',
          passphrase: 'test-passphrase',
          timestamp: 1_750_000_000,
          sign: 'oIVmUHwMPskagvu2CkTKCARKTbYCl7GTIlaAg5wTJHQ=',
        },
      ],
    });

    const loginSuccess = { data: JSON.stringify({ event: 'login', code: '0', msg: '' }) } as MessageEvent;
    socket.onMessage(loginSuccess);
    socket.onMessage(loginSuccess);

    expect(sentJson(send).filter((payload) => payload.op === 'subscribe')).toEqual([
      {
        op: 'subscribe',
        args: [
          { channel: 'account' },
          { channel: 'positions', instType: 'SWAP' },
          { channel: 'orders', instType: 'SWAP' },
        ],
      },
      {
        op: 'subscribe',
        args: [
          { channel: 'account' },
          { channel: 'positions', instType: 'SWAP' },
          { channel: 'orders', instType: 'SWAP' },
        ],
      },
    ]);
  });

  test('Bitget signs user verification and resubscribes all private channels after login', async () => {
    const parent = {
      ...commonParent(),
      name: 'bitget',
      apiProductType: 'usdt-futures',
    } as unknown as BitgetExchange;
    const socket = new BitgetPrivateWebsocket(parent);
    const send = attachOpenSocket(socket);

    await (socket as any).auth();

    expect(sentJson(send)).toContainEqual({
      op: 'login',
      args: [
        {
          apiKey: 'test-key',
          passphrase: 'test-passphrase',
          timestamp: 1_750_000_000,
          sign: 'h9xi0HXzDQJGx4kPdYZyMf4wwdBmEnpOWDq1zCxfWZE=',
        },
      ],
    });

    const loginSuccess = { data: JSON.stringify({ event: 'login', code: '0', msg: '' }) } as MessageEvent;
    socket.onMessage(loginSuccess);
    socket.onMessage(loginSuccess);

    const expectedArgs = [
      { instType: 'USDT-FUTURES', channel: 'account', coin: 'default' },
      { instType: 'USDT-FUTURES', channel: 'positions', instId: 'default' },
      { instType: 'USDT-FUTURES', channel: 'orders', instId: 'default' },
      { instType: 'USDT-FUTURES', channel: 'orders-algo', instId: 'default' },
    ];
    expect(sentJson(send).filter((payload) => payload.op === 'subscribe')).toEqual([
      { op: 'subscribe', args: expectedArgs },
      { op: 'subscribe', args: expectedArgs },
    ]);
  });
});
