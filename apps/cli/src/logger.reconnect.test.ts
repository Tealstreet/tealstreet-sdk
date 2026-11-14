import { once } from 'node:events';
import { createServer } from 'node:http';

import { afterEach, expect, it, vi } from 'vitest';

import { HOWWebSocket } from '@tealstreet/http-over-websocket/src/HOWWebSocket';

// These tests isolate terminal/file formatting; profile routing has real-FS tests.
vi.mock('./storage-context', () => ({
  getStorageDir: () => '/tmp/cli-logger-test',
  onStorageOwnerChange: () => () => {},
}));

const fileLines: string[] = [];
vi.mock('fs', async (importOriginal) => ({
  ...(await importOriginal<typeof import('fs')>()),
  existsSync: () => true,
  writeFileSync: () => undefined,
  appendFileSync: (_path: unknown, line: string) => void fileLines.push(line),
}));
const statuses: string[] = [];
vi.mock('./status-line', () => ({ setStatus: (message: string) => void statuses.push(message) }));

let restore: (() => void) | undefined;
afterEach(() => {
  restore?.();
  fileLines.length = 0;
  statuses.length = 0;
});

async function captureTerminal() {
  const terminal: unknown[][] = [];
  const original = { log: console.log, error: console.error, warn: console.warn };
  console.log = console.error = console.warn = (...args: unknown[]) => void terminal.push(args);
  vi.resetModules();
  const logger = await import('./logger');
  logger.interceptConsole();
  restore = () => {
    logger.restoreConsole();
    Object.assign(console, original);
  };
  return terminal;
}

it('keeps reconnect diagnostics in cli.log while real failures still reach the terminal and status bar', async () => {
  const terminal = await captureTerminal();
  const diagnostics = [
    '[ReconnectingWebSocket] reconnect() requested',
    'RWS> next delay 1000',
    '[HOWWebSocket] WebSocket connection disconnected, reconnecting...',
    '[HOWWebSocket] emitting disconnect event',
  ];
  for (const message of diagnostics) console.info(message);
  expect(terminal).toEqual([]);
  expect(statuses).toEqual([]);
  for (const message of diagnostics) expect(fileLines.join('')).toContain(message);

  console.error('[HOWWebSocket] Reconnect retries exhausted');
  console.error('[HOW] Tunnel auth failed: ExpiredSignature');
  console.error('Bybit order outcome is unknown. Check Open Orders');
  expect(terminal).toHaveLength(3);
  expect(statuses).toHaveLength(3);
});

it.each([401, 403])('shows a real HTTP %i tunnel upgrade refusal in the terminal and status bar', async (status) => {
  const terminal = await captureTerminal();
  const server = createServer();
  server.on('upgrade', (_request, peer) => {
    peer.end(`HTTP/1.1 ${status} Refused\r\nConnection: close\r\nContent-Length: 0\r\n\r\n`);
  });
  server.listen(0, '127.0.0.1');
  await once(server, 'listening');
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('Expected a loopback TCP address');
  const socket = new HOWWebSocket(`ws://127.0.0.1:${address.port}/tunnel`);
  try {
    socket.connectAndSubscribe();
    const message = `[HOW] Tunnel auth failed: WebSocket upgrade refused (HTTP ${status})`;
    await vi.waitFor(() => expect(statuses).toEqual([message]));
    expect(terminal).toHaveLength(1);
    expect(terminal.flat().join(' ')).toContain(message);
    expect(fileLines.join('')).toContain('WebSocket connection disconnected, reconnecting...');
  } finally {
    socket.dispose();
    await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  }
});
