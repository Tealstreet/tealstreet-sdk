import { once } from 'node:events';
import { createServer } from 'node:http';

import { afterEach, expect, it, vi } from 'vitest';

import { HOWWebSocket } from './HOWWebSocket';
import { HOWWebSocketAuthState } from './types';

afterEach(() => vi.restoreAllMocks());

async function withUpgradeRefusal(status: number, check: (socket: HOWWebSocket) => Promise<void>) {
  const server = createServer();
  server.on('upgrade', (_request, peer) => {
    peer.end(`HTTP/1.1 ${status} Refused\r\nConnection: close\r\nContent-Length: 0\r\n\r\n`);
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Expected a loopback TCP address');
  const socket = new HOWWebSocket(`ws://127.0.0.1:${address.port}/tunnel?token=fixture-private-token`);
  try {
    socket.connectAndSubscribe();
    await check(socket);
  } finally {
    socket.dispose();
    await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  }
}

it.each([401, 403])('surfaces an HTTP %i upgrade refusal through the tunnel auth error path', async (status) => {
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  const info = vi.spyOn(console, 'info').mockImplementation(() => {});
  await withUpgradeRefusal(status, async (socket) => {
    await vi.waitFor(() => {
      expect(error).toHaveBeenCalledExactlyOnceWith(
        `[HOW] Tunnel auth failed: WebSocket upgrade refused (HTTP ${status})`,
      );
    });
    expect(socket.authState).toBe(HOWWebSocketAuthState.Bad);
    expect(info.mock.calls.flat().join(' ')).not.toContain(`Unexpected server response: ${status}`);
    expect(error.mock.calls.flat().join(' ')).not.toContain('fixture-private-token');
  });
});

it('keeps a non-auth upgrade failure informational while scheduling a reconnect', async () => {
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  const info = vi.spyOn(console, 'info').mockImplementation(() => {});
  await withUpgradeRefusal(503, async (socket) => {
    await vi.waitFor(() => {
      expect(info).toHaveBeenCalledWith('[HOWWebSocket] WebSocket error: Unexpected server response: 503');
    });
    expect(socket.ws?.loopState.phase).toBe('retry-scheduled');
    expect(socket.authState).toBe(HOWWebSocketAuthState.Unknown);
    expect(error).not.toHaveBeenCalled();
  });
});

it('preserves the RPC authentication failure control', async () => {
  const error = vi.spyOn(console, 'error').mockImplementation(() => {});
  const socket = new HOWWebSocket('wss://tunnel.test/tunnel');
  await socket.onMessage(new MessageEvent('message', { data: '{"type":"rpc.auth","error":"ExpiredSignature"}' }));
  expect(error).toHaveBeenCalledExactlyOnceWith('[HOW] Tunnel auth failed: ExpiredSignature');
  expect(socket.authState).toBe(HOWWebSocketAuthState.Bad);
});
