import childProcess from 'node:child_process';
import { Socket } from 'node:net';

import { describe, expect, it, vi } from 'vitest';

import { UnitTransportGuard, unitTransportGuard } from '../../../config/test/transportGuard';

describe('unit transport isolation', () => {
  it('remembers denied transport even when application code catches it', () => {
    const guard = new UnitTransportGuard();
    for (const kind of ['fetch', 'XMLHttpRequest.send', 'socket.connect']) {
      try {
        guard.deny(kind);
      } catch {
        /* the production catch must not hide the attempt */
      }
    }
    expect(() => guard.assertClean()).toThrow('3 attempts');
    guard.reset();
    expect(() => guard.assertClean()).not.toThrow();
  });

  it.each([
    [{ host: 'exchange.invalid', port: 443 }],
    [{ host: '127.0.0.1', port: 443 }],
    [443, '::1'],
    ['/tmp/socket'],
    [{ path: '/tmp/socket', host: '127.0.0.1', port: 443 }],
  ])('denies sockets without delegating: %j', (...args) => {
    const guard = new UnitTransportGuard();
    const delegate = vi.fn();
    expect(() => guard.connect(args, delegate)).toThrow('socket.connect');
    expect(delegate).not.toHaveBeenCalled();
    expect(() => guard.assertClean()).toThrow();
  });

  it('admits only the exact explicit fixture port across socket overloads', () => {
    const guard = new UnitTransportGuard();
    const revoke = guard.allowLoopbackPort(12345);
    const delegate = vi.fn(() => 'local fixture');
    for (const args of [
      [{ host: '127.0.0.1', port: 12345 }],
      [{ host: '::1', port: '12345' }],
      [[{ host: '127.0.0.1', port: 12345 }, () => undefined]],
      [12345, '127.0.0.1'],
    ])
      expect(guard.connect(args, delegate)).toBe('local fixture');
    expect(delegate).toHaveBeenCalledTimes(4);
    for (const args of [
      [12346, '127.0.0.1'],
      [12345, 'localhost'],
      [12345, 'exchange.invalid'],
    ]) {
      expect(() => guard.connect(args, delegate)).toThrow();
    }
    expect(delegate).toHaveBeenCalledTimes(4);
    revoke();
    expect(() => guard.connect([12345, '127.0.0.1'], delegate)).toThrow();
    expect(delegate).toHaveBeenCalledTimes(4);
  });

  it('an old revoked lease cannot remove a later fixture allowance', () => {
    const guard = new UnitTransportGuard();
    const old = guard.allowLoopbackPort(12345);
    guard.reset();
    const current = guard.allowLoopbackPort(12345);
    old();
    expect(guard.connect([12345, '127.0.0.1'], () => true)).toBe(true);
    current();
    expect(() => guard.connect([12345, '127.0.0.1'], () => true)).toThrow();
  });

  it.each([0, -1, 65536, Infinity, NaN, 1.5])('rejects invalid fixture port %s', (port) => {
    expect(() => new UnitTransportGuard().allowLoopbackPort(port)).toThrow('Invalid fixture port');
  });

  it('keeps the setup boundary across mock reset and restore', () => {
    const connect = Socket.prototype.connect;
    vi.spyOn(Socket.prototype, 'connect').mockImplementation(() => {
      throw new Error('local mock');
    });
    vi.restoreAllMocks();
    expect(Socket.prototype.connect).toBe(connect);
    vi.resetAllMocks();
    expect(() => fetch('https://exchange.invalid')).toThrow('Unstubbed unit-test transport: fetch');
    expect(() => unitTransportGuard.assertClean()).toThrow('fetch');
    unitTransportGuard.reset();
  });

  it('covers synchronous XHR before happy-dom can spawn its HTTP worker', () => {
    const xhr = new XMLHttpRequest();
    xhr.open('GET', 'https://exchange.invalid', false);
    expect(() => xhr.send()).toThrow('Unstubbed unit-test transport: XMLHttpRequest.send');
    expect(() => unitTransportGuard.assertClean()).toThrow('XMLHttpRequest.send');
    unitTransportGuard.reset();
  });

  it('also denies synchronous XHR in a sibling window without spawning a process', () => {
    const spawn = vi.spyOn(childProcess, 'execFileSync').mockImplementation(() => {
      throw new Error('Unexpected synchronous HTTP subprocess');
    });
    const sibling = window.open('about:blank') as Window & typeof globalThis;
    try {
      const xhr = new sibling.XMLHttpRequest();
      xhr.open('GET', 'https://exchange.invalid', false);
      expect(() => xhr.send()).toThrow('Unstubbed unit-test transport: XMLHttpRequest.send');
      expect(() => unitTransportGuard.assertClean()).toThrow('XMLHttpRequest.send');
      expect(spawn).not.toHaveBeenCalled();
    } finally {
      unitTransportGuard.reset();
      sibling.close();
    }
  });
});
