import { describe, expect, it } from 'vitest';

import { classifyLog, isOperational } from './log-classify';

describe('classifyLog', () => {
  describe('operational (console.log noise → file only)', () => {
    it('classifies the trackLastReceived ping/reconnect diagnostics as operational', () => {
      expect(classifyLog('[trackLastReceived] bitunix last received message 6033ms ago, sending ping', 'log')).toBe(
        'operational',
      );
      expect(classifyLog('[trackLastReceived] bybit last received message 21000ms ago, reconnecting', 'log')).toBe(
        'operational',
      );
    });

    it('classifies reconnecting-websocket / keepalive noise as operational', () => {
      expect(classifyLog('[BaseReconnectingWebSocket] reconnect attempt 3', 'log')).toBe('operational');
      expect(classifyLog('setting reconnecting = true', 'log')).toBe('operational');
      expect(classifyLog('[HOW] Latency 42ms', 'log')).toBe('operational');
      expect(classifyLog('sending keepalive frame', 'log')).toBe('operational');
    });

    it('classifies the bitunix balance-dump console.info line as operational', () => {
      expect(classifyLog('bitunix: received balance message, but we will fetch it from the exchange', 'log')).toBe(
        'operational',
      );
    });

    it('does NOT silence the same content when it arrives via console.error', () => {
      // A genuine error is never demoted to operational, even if it mentions a ping/reconnect.
      expect(classifyLog('[trackLastReceived] reconnect failed permanently', 'error')).not.toBe('operational');
    });
  });

  describe('info (general console.log)', () => {
    it('classifies ordinary log lines as info', () => {
      expect(classifyLog('Connected to bybit', 'log')).toBe('info');
      expect(classifyLog('Markets loaded', 'log')).toBe('info');
    });
  });

  describe('warn', () => {
    it('classifies console.warn as warn', () => {
      expect(classifyLog('Listen key refresh slow', 'warn')).toBe('warn');
    });
  });

  describe('error', () => {
    it('classifies console.error as error', () => {
      expect(classifyLog('Order rejected: insufficient margin', 'error')).toBe('error');
    });
  });

  describe('critical (escalated warn/error a trader must not miss)', () => {
    it('escalates auth failures', () => {
      expect(classifyLog('Authentication failed. Your token may have expired.', 'error')).toBe('critical');
      expect(classifyLog('HTTP 401 Unauthorized', 'error')).toBe('critical');
      expect(classifyLog('token invalid', 'warn')).toBe('critical');
    });

    it('escalates liquidation + repeated-failure events', () => {
      expect(classifyLog('Position liquidated on BTCUSDT', 'error')).toBe('critical');
      expect(classifyLog('Max consecutive failures reached', 'error')).toBe('critical');
    });

    it('is case-insensitive', () => {
      expect(classifyLog('AUTHENTICATION FAILED', 'error')).toBe('critical');
      expect(classifyLog('LIQUIDATED', 'warn')).toBe('critical');
    });
  });
});

describe('isOperational', () => {
  it('matches operational prefixes regardless of console method context', () => {
    expect(isOperational('[trackLastReceived] sending ping')).toBe(true);
    expect(isOperational('Connected to bybit')).toBe(false);
  });
});
