import { Socket } from 'node:net';

import { afterAll, afterEach, beforeEach, vi } from 'vitest';
import WS from 'ws';

import { unitTransportGuard } from './transportGuard';

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

const originalConnect = Socket.prototype.connect;
// happy-dom gives each window a subclass. Guard their shared implementation,
// otherwise a sibling window's synchronous XHR can spawn its own HTTP worker.
const xhrPrototype = (() => {
  if (typeof XMLHttpRequest === 'undefined') return undefined;
  let prototype = XMLHttpRequest.prototype;
  while (prototype && !Object.prototype.hasOwnProperty.call(prototype, 'send')) {
    prototype = Object.getPrototypeOf(prototype);
  }
  return prototype;
})();
const originalXhrSend = xhrPrototype?.send;
const guardedConnect = function (this: Socket, ...args: unknown[]) {
  return unitTransportGuard.connect(args, () => Reflect.apply(originalConnect, this, args));
} as typeof Socket.prototype.connect;
const guardedXhrSend: typeof XMLHttpRequest.prototype.send = () => unitTransportGuard.deny('XMLHttpRequest.send');

const armTransportGuard = () => {
  // Check first: never erase a late attempt from a previous cleanup/import.
  unitTransportGuard.assertClean();
  unitTransportGuard.reset();
  global.fetch = vi.fn(() => unitTransportGuard.deny('fetch'));
  Socket.prototype.connect = guardedConnect;
  if (xhrPrototype) xhrPrototype.send = guardedXhrSend;
};

armTransportGuard();
beforeEach(armTransportGuard);
afterEach(() => {
  try {
    // A venue catch handler must not hide an accidental real request.
    unitTransportGuard.assertClean();
  } finally {
    unitTransportGuard.reset();
  }
});
afterAll(() => {
  try {
    unitTransportGuard.assertClean();
  } finally {
    vi.restoreAllMocks();
    Socket.prototype.connect = originalConnect;
    if (xhrPrototype && originalXhrSend) xhrPrototype.send = originalXhrSend;
  }
});

const originalConsole = { ...console };
global.console = {
  ...originalConsole,
  log: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  info: vi.fn(),
  debug: vi.fn(),
};
