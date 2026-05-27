import { vi } from 'vitest';

if (typeof window === 'undefined') {
  Object.defineProperty(global, 'window', {
    value: {},
    writable: true,
  });
}

if (!process.env.NODE_ENV) {
  process.env.NODE_ENV = 'test';
}

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
