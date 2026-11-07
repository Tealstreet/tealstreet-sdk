import { afterEach, expect, it, vi } from 'vitest';

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

it('keeps reconnect diagnostics in cli.log while real failures still reach the terminal and status bar', async () => {
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
