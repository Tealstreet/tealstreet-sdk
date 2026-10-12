import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  WebhookRelayClient,
  type RelaySocket,
  type RunWebhookBody,
  type WebhookRunContext,
} from './WebhookRelayClient';

class FakeSocket implements RelaySocket {
  sent: Array<Record<string, unknown>> = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  closed = false;
  send(data: string) {
    this.sent.push(JSON.parse(data));
  }
  close() {
    this.closed = true;
    this.onclose?.();
  }
  receive(frame: Record<string, unknown>) {
    this.onmessage?.({ data: JSON.stringify(frame) });
  }
  ofType(type: string) {
    return this.sent.filter((f) => f.type === type);
  }
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('WebhookRelayClient', () => {
  let sockets: FakeSocket[];
  let runBody: ReturnType<typeof vi.fn<RunWebhookBody>>;
  let notice: {
    info: ReturnType<typeof vi.fn<(message: string) => void>>;
    warn: ReturnType<typeof vi.fn<(message: string) => void>>;
  };
  let relay: WebhookRelayClient;

  const socket = () => sockets[sockets.length - 1];
  const connectAndAuth = () => {
    relay.start();
    socket().onopen?.();
    socket().receive({ type: 'rpc.auth', status: 'success' });
    socket().receive({ type: 'webhook.registered', listenerId: 'cli-me', claimedListenerId: 'cli-me' });
  };

  beforeEach(() => {
    sockets = [];
    runBody = vi.fn<RunWebhookBody>(async () => {});
    notice = { info: vi.fn<(message: string) => void>(), warn: vi.fn<(message: string) => void>() };
    relay = new WebhookRelayClient({
      listenerId: 'cli-me',
      kind: 'cli',
      label: 'Test CLI',
      getAccounts: () => ['main', 'alt'],
      version: '9.9.9',
      getToken: () => 'tok',
      runBody,
      notice,
      mirror: () => {},
      createSocket: () => {
        const s = new FakeSocket();
        sockets.push(s);
        return s;
      },
    });
  });

  afterEach(() => {
    relay.stop();
    vi.useRealTimers();
  });

  it('authenticates, then registers this CLI with its account names', () => {
    connectAndAuth();
    expect(socket().sent[0]).toEqual({ type: 'rpc.auth', token: 'tok', source: 'cli' });
    expect(socket().ofType('webhook.register')[0]).toMatchObject({
      listenerId: 'cli-me',
      kind: 'cli',
      accounts: ['main', 'alt'],
      version: '9.9.9',
    });
    expect(relay.holdsClaim).toBe(true);
  });

  it('runs a delivery only after the relay accepts the received ack, then acks done with output', async () => {
    runBody.mockImplementation(async (_body: string, ctx: WebhookRunContext) => {
      ctx.printSink?.('success', 'Placed buy 0.1 BTCUSDT');
    });
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd1', body: 'buy 0.1 BTCUSDT' });
    expect(socket().ofType('webhook.ack')).toEqual([{ type: 'webhook.ack', deliveryId: 'd1', status: 'received' }]);
    expect(runBody).not.toHaveBeenCalled();

    socket().receive({ type: 'webhook.acked', deliveryId: 'd1', status: 'received', accepted: true });
    await flush();

    expect(runBody).toHaveBeenCalledWith('buy 0.1 BTCUSDT', expect.objectContaining({ source: 'webhook' }));
    expect(socket().ofType('webhook.ack')[1]).toEqual({
      type: 'webhook.ack',
      deliveryId: 'd1',
      status: 'done',
      output: 'Placed buy 0.1 BTCUSDT\n',
    });
  });

  it('never runs a delivery the relay refuses', async () => {
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd2', body: 'sell 1 ETHUSDT' });
    socket().receive({ type: 'webhook.acked', deliveryId: 'd2', status: 'received', accepted: false });
    await flush();
    expect(runBody).not.toHaveBeenCalled();
    expect(notice.warn).toHaveBeenCalled();
  });

  it('never runs a delivery whose received ack never arrives', async () => {
    vi.useFakeTimers();
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd3', body: 'buy 1 SOLUSDT' });
    await vi.advanceTimersByTimeAsync(10_001);
    expect(runBody).not.toHaveBeenCalled();
  });

  it('ignores a duplicate delivery of the same id', async () => {
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd4', body: 'buy' });
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd4', body: 'buy' });
    expect(socket().ofType('webhook.ack')).toHaveLength(1);
  });

  it('acks failed when a step printed an error', async () => {
    runBody.mockImplementation(async (_body: string, ctx: WebhookRunContext) => {
      ctx.printSink?.('error', 'insufficient margin');
    });
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd5', body: 'buy 100 BTCUSDT' });
    socket().receive({ type: 'webhook.acked', deliveryId: 'd5', status: 'received', accepted: true });
    await flush();
    expect(socket().ofType('webhook.ack')[1]).toMatchObject({ status: 'failed', output: 'insufficient margin\n' });
  });

  it('reconnects after the socket drops, and waits for a NEW token after a refused sign-in', async () => {
    vi.useFakeTimers();
    connectAndAuth();
    socket().close();
    await vi.advanceTimersByTimeAsync(1_001);
    expect(sockets).toHaveLength(2);

    socket().onopen?.();
    socket().receive({ type: 'rpc.auth', error: 'expired' });
    expect(relay.connection).toBe('unauthorized');
    // Same (rejected) token: no new socket, however long we wait.
    await vi.advanceTimersByTimeAsync(180_000);
    expect(sockets).toHaveLength(2);
  });

  it('says nothing on a plain (re)register, only when the claim moves', () => {
    connectAndAuth();
    expect(notice.info).not.toHaveBeenCalled();
  });

  it('tells the user when the claim moves away and back', () => {
    connectAndAuth();
    socket().receive({ type: 'webhook.claim_changed', claimedListenerId: 'web-1', previousListenerId: 'cli-me' });
    expect(notice.warn).toHaveBeenCalledWith(expect.stringContaining('another listener'));
    socket().receive({ type: 'webhook.claim_changed', claimedListenerId: 'cli-me', previousListenerId: 'web-1' });
    expect(notice.info).toHaveBeenLastCalledWith(expect.stringContaining('now receives'));
  });

  it('runs an alert when it is replayed after an unconfirmed first delivery', async () => {
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd9', body: 'buy 1 BTCUSDT' });
    socket().receive({ type: 'webhook.acked', deliveryId: 'd9', status: 'received', accepted: false });
    await flush();
    expect(runBody).not.toHaveBeenCalled();

    socket().receive({ type: 'webhook.deliver', deliveryId: 'd9', body: 'buy 1 BTCUSDT' });
    socket().receive({ type: 'webhook.acked', deliveryId: 'd9', status: 'received', accepted: true });
    await flush();
    expect(runBody).toHaveBeenCalledTimes(1);
  });

  it('re-sends a result the relay never acknowledged after the next register', async () => {
    vi.useFakeTimers();
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd10', body: 'buy 1 BTCUSDT' });
    socket().receive({ type: 'webhook.acked', deliveryId: 'd10', status: 'received', accepted: true });
    await vi.advanceTimersByTimeAsync(0);
    socket().close();
    await vi.advanceTimersByTimeAsync(1_001);

    connectAndAuth();
    expect(socket().ofType('webhook.ack')).toEqual([expect.objectContaining({ deliveryId: 'd10', status: 'done' })]);

    socket().receive({ type: 'webhook.acked', deliveryId: 'd10', status: 'done', accepted: true });
    socket().receive({ type: 'webhook.registered', listenerId: 'cli-me', claimedListenerId: 'cli-me' });
    expect(socket().ofType('webhook.ack')).toHaveLength(1);
  });

  it('turns the relay remaining time into a deadline on this clock', async () => {
    connectAndAuth();
    socket().receive({ type: 'webhook.deliver', deliveryId: 'd11', body: 'buy 1 BTCUSDT', remainingMs: 45_000 });
    const before = Date.now();
    socket().receive({ type: 'webhook.acked', deliveryId: 'd11', status: 'received', accepted: true });
    await flush();
    const deadline = (runBody.mock.calls[0][1] as { expiresAt: number }).expiresAt;
    expect(deadline).toBeGreaterThanOrEqual(before + 44_000);
    expect(deadline).toBeLessThan(before + 46_000);
  });

  it('retries registering when the relay could not register the listener', async () => {
    vi.useFakeTimers();
    relay.start();
    socket().onopen?.();
    socket().receive({ type: 'rpc.auth', status: 'success' });
    socket().receive({ type: 'webhook.error', code: 'unavailable', message: 'could not register this listener' });
    await vi.advanceTimersByTimeAsync(30_001);
    expect(socket().ofType('webhook.register')).toHaveLength(2);
  });
});
