import { afterEach, describe, expect, it, vi } from 'vitest';

import { BaseWebSocket } from '../exchanges/base.ws';

// A private socket that re-opens may have missed fills and cancels while it was
// down, on any socket path; the open listener every socket gets is what resyncs.

class TestSocket extends BaseWebSocket<any> {
  connectAndSubscribe = () => undefined;
  onOpen = () => undefined;
  onMessage = () => undefined;
  onClose = () => undefined;
  open() {
    (this as any).trackConnected();
  }
}

const exchange = (options: Record<string, unknown> = { key: 'k' }) => {
  const parent: any = {
    name: 'test',
    options,
    isDisposed: false,
    log: vi.fn(),
    fetchAndSetPrivateSnapshot: vi.fn(async () => undefined),
  };
  const privateSocket = new TestSocket(parent);
  const publicSocket = new TestSocket(parent);
  parent.getPrivateSockets = () => [privateSocket];
  return { parent, privateSocket, publicSocket };
};

describe('BaseWebSocket re-open resync', () => {
  afterEach(() => vi.useRealTimers());

  it('resyncs private state on a re-open, not on the first open', async () => {
    const { parent, privateSocket } = exchange();

    privateSocket.open();
    expect(parent.fetchAndSetPrivateSnapshot).not.toHaveBeenCalled();
    privateSocket.open();
    await vi.waitFor(() => expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledWith(10));
  });

  it("never resyncs for a keyed account's public socket, a keyless account, or a disposed one", () => {
    const keyed = exchange();
    keyed.publicSocket.open();
    keyed.publicSocket.open();

    const keyless = exchange({ key: '' });
    keyless.privateSocket.open();
    keyless.privateSocket.open();

    const disposed = exchange();
    disposed.privateSocket.open();
    disposed.parent.isDisposed = true;
    disposed.privateSocket.open();

    for (const { parent } of [keyed, keyless, disposed]) {
      expect(parent.fetchAndSetPrivateSnapshot).not.toHaveBeenCalled();
    }
  });

  it('runs one follow-up, not a stack, for re-opens during a running resync', async () => {
    const { parent, privateSocket } = exchange();
    let finish!: () => void;
    parent.fetchAndSetPrivateSnapshot.mockImplementationOnce(() => new Promise<void>((resolve) => (finish = resolve)));

    privateSocket.open();
    privateSocket.open();
    privateSocket.open();
    privateSocket.open();
    expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledTimes(1);

    // The re-opens mid-run may have missed more than that run saw: exactly one more.
    finish();
    await vi.waitFor(() => expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledTimes(2));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledTimes(2);
  });

  it('resyncs once when two private sockets of one exchange re-open together', async () => {
    vi.useFakeTimers({ toFake: ['Date'], now: 1_000_000 });
    const { parent, privateSocket } = exchange();
    const business = new (class extends TestSocket {
      protected get resyncsPrivateStateOnReopen() {
        return true;
      }
    })(parent);

    privateSocket.open();
    business.open();
    privateSocket.open();
    business.open();
    expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledTimes(1);

    // The same socket dropping again inside the window may have missed more.
    await new Promise((resolve) => setTimeout(resolve, 0));
    privateSocket.open();
    expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledTimes(2);

    vi.setSystemTime(1_002_000);
    await new Promise((resolve) => setTimeout(resolve, 0));
    business.open();
    expect(parent.fetchAndSetPrivateSnapshot).toHaveBeenCalledTimes(3);
  });
});
