import { vi } from 'vitest';
import WS from 'ws';

if (typeof window === 'undefined') {
  Object.defineProperty(global, 'window', {
    value: {},
    writable: true,
  });
}

if (!process.env.NODE_ENV) {
  process.env.NODE_ENV = 'test';
}

// Add WebSocket polyfill for Node.js tests
global.WebSocket = WS as any;

global.fetch = vi.fn();

const originalConsole = { ...console };
global.console = {
  ...originalConsole,
  log: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
  debug: vi.fn(),
};
