import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { ExchangeHealthMonitor } from './ExchangeHealthMonitor';

describe('ExchangeHealthMonitor', () => {
  let monitor: ExchangeHealthMonitor;

  beforeEach(() => {
    vi.useFakeTimers();
    monitor = new ExchangeHealthMonitor('test');
  });

  afterEach(() => {
    monitor.dispose();
    vi.useRealTimers();
  });

  describe('constructor', () => {
    it('should initialize with healthy status', () => {
      expect(monitor.isHealthy()).toBe(true);
      expect(monitor.getState().status).toBe('healthy');
    });

    it('should initialize with empty error counts', () => {
      const state = monitor.getState();
      expect(Object.keys(state.errorCounts).length).toBe(0);
    });

    it('should initialize with empty recent errors', () => {
      expect(monitor.getState().recentErrors).toEqual([]);
    });

    it('should accept custom max recent errors option', () => {
      const customMonitor = new ExchangeHealthMonitor('test', { maxRecentErrors: 5 });
      // Add 10 errors
      for (let i = 0; i < 10; i++) {
        customMonitor.processMessage('invalid api key');
      }
      expect(customMonitor.getState().recentErrors.length).toBe(5);
      customMonitor.dispose();
    });

    it('should merge exchange-specific patterns for known exchanges', () => {
      const binanceMonitor = new ExchangeHealthMonitor('binance');
      // Binance-specific error code should be recognized
      const category = binanceMonitor.processMessage('error code -2014');
      expect(category).toBe('auth_invalid');
      binanceMonitor.dispose();
    });

    it('classifies Bitunix per-symbol OpenAPI and agreement rejections as order warnings', () => {
      const bitunixMonitor = new ExchangeHealthMonitor('bitunix');

      expect(
        bitunixMonitor.processMessage('This trading pair does not currently support trading via OpenAPI.', true),
      ).toBe('order_warning');
      expect(
        bitunixMonitor.processMessage(
          'Stock Perpetual trading permissions not activated. Please update App or use Web to sign.',
          true,
        ),
      ).toBe('order_warning');
      expect(bitunixMonitor.hasAuthError()).toBe(false);
      bitunixMonitor.dispose();
    });

    it('classifies Bybit agreement rejections as order warnings for HyroTrader', () => {
      const hyrotraderMonitor = new ExchangeHealthMonitor('hyrotrader');

      expect(
        hyrotraderMonitor.processMessage(
          'Bybit Trading Terms not accepted for this contract. For HyroTrader, wait briefly and retry.',
          true,
        ),
      ).toBe('order_warning');
      expect(
        hyrotraderMonitor.processMessage('retCode: 110125 You must agree to the Trading Terms before trading', true),
      ).toBe('order_warning');
      expect(hyrotraderMonitor.hasAuthError()).toBe(false);
      hyrotraderMonitor.dispose();
    });
  });

  describe('processMessage', () => {
    it('should detect auth_invalid errors', () => {
      const category = monitor.processMessage('invalid api key');
      expect(category).toBe('auth_invalid');
    });

    it('should detect rate_limit errors', () => {
      const category = monitor.processMessage('rate limit exceeded');
      expect(category).toBe('rate_limit');
    });

    it('should detect network errors', () => {
      const category = monitor.processMessage('ECONNREFUSED');
      expect(category).toBe('network');
    });

    it('should detect time_sync errors', () => {
      const category = monitor.processMessage('recv window error');
      expect(category).toBe('time_sync');
    });

    it('does not classify routine order expiry as expired authentication', () => {
      expect(monitor.processMessage('Order expired before it could be filled')).toBeNull();
      expect(monitor.processMessage('This request has expired - `expires` is in the past.')).toBeNull();
      expect(monitor.processMessage('good_til_time order expired normally')).toBeNull();
    });

    it('requires authentication context for expiry errors', () => {
      expect(monitor.processMessage('API key has expired')).toBe('auth_expired');
      expect(monitor.processMessage('session credential expired')).toBe('auth_expired');
      expect(monitor.processMessage('request signature has expired')).toBe('auth_expired');
    });

    it('requires synchronization context around timestamp and clock errors', () => {
      expect(monitor.processMessage('order timestamp expired')).toBeNull();
      expect(monitor.processMessage('timestamp for this request is ahead of server time')).toBe('time_sync');
      expect(monitor.processMessage('local clock drift exceeds tolerance')).toBe('time_sync');
    });

    it('should detect exchange_down errors', () => {
      const category = monitor.processMessage('service unavailable');
      expect(category).toBe('exchange_down');
    });

    it('should return null for non-matching messages', () => {
      const category = monitor.processMessage('everything is fine');
      expect(category).toBeNull();
    });

    it('should return unknown for explicit errors that do not match patterns', () => {
      const category = monitor.processMessage('something went wrong', true);
      expect(category).toBe('unknown');
    });

    it('should handle empty messages', () => {
      const category = monitor.processMessage('');
      expect(category).toBeNull();
    });

    it('should handle null-ish messages', () => {
      const category = monitor.processMessage(null as any);
      expect(category).toBeNull();
    });

    it('should be case insensitive for string patterns', () => {
      const category = monitor.processMessage('INVALID API KEY');
      expect(category).toBe('auth_invalid');
    });
  });

  describe('health status transitions', () => {
    it('should transition to error on critical errors', () => {
      monitor.processMessage('invalid api key'); // critical
      expect(monitor.getState().status).toBe('error');
    });

    it('should transition to degraded on warning errors from healthy', () => {
      monitor.processMessage('rate limit exceeded'); // warning
      expect(monitor.getState().status).toBe('degraded');
    });

    it('should stay in error status after critical error', () => {
      monitor.processMessage('invalid api key'); // critical
      monitor.processMessage('rate limit exceeded'); // warning
      expect(monitor.getState().status).toBe('error');
    });

    it('should auto-recover from degraded after recovery window', () => {
      monitor.processMessage('rate limit exceeded'); // warning
      expect(monitor.getState().status).toBe('degraded');

      vi.advanceTimersByTime(60_000); // default recovery window

      expect(monitor.getState().status).toBe('healthy');
    });

    it('should not auto-recover from error status', () => {
      monitor.processMessage('invalid api key'); // critical
      expect(monitor.getState().status).toBe('error');

      vi.advanceTimersByTime(60_000);

      expect(monitor.getState().status).toBe('error');
    });

    it('should use custom recovery window', () => {
      const customMonitor = new ExchangeHealthMonitor('test', { recoveryWindowMs: 5000 });
      customMonitor.processMessage('rate limit exceeded');
      expect(customMonitor.getState().status).toBe('degraded');

      vi.advanceTimersByTime(5000);

      expect(customMonitor.getState().status).toBe('healthy');
      customMonitor.dispose();
    });
  });

  describe('error tracking', () => {
    it('should increment error counts', () => {
      monitor.processMessage('invalid api key');
      monitor.processMessage('invalid api key');
      expect(monitor.getErrorCount('auth_invalid')).toBe(2);
    });

    it('should track different error categories separately', () => {
      monitor.processMessage('invalid api key');
      monitor.processMessage('rate limit exceeded');
      expect(monitor.getErrorCount('auth_invalid')).toBe(1);
      expect(monitor.getErrorCount('rate_limit')).toBe(1);
    });

    it('should store last error', () => {
      monitor.processMessage('invalid api key');
      const lastError = monitor.getLastError();
      expect(lastError).toBeDefined();
      expect(lastError?.category).toBe('auth_invalid');
      expect(lastError?.message).toBe('invalid api key');
    });

    it('should update last error on new error', () => {
      monitor.processMessage('invalid api key');
      monitor.processMessage('rate limit exceeded');
      const lastError = monitor.getLastError();
      expect(lastError?.category).toBe('rate_limit');
    });

    it('should limit recent errors to maxRecentErrors', () => {
      for (let i = 0; i < 15; i++) {
        monitor.processMessage(`error ${i}`, true);
      }
      expect(monitor.getState().recentErrors.length).toBe(10); // default max
    });
  });

  describe('hasAuthError', () => {
    it('should return false when no auth errors', () => {
      expect(monitor.hasAuthError()).toBe(false);
    });

    it('should return true for auth_invalid', () => {
      monitor.processMessage('invalid api key');
      expect(monitor.hasAuthError()).toBe(true);
    });

    it('should return true for auth_expired', () => {
      monitor.processMessage('token expired');
      expect(monitor.hasAuthError()).toBe(true);
    });

    it('should return true for auth_permissions', () => {
      monitor.processMessage('permission denied');
      expect(monitor.hasAuthError()).toBe(true);
    });
  });

  describe('hasError', () => {
    it('should return false for category with no errors', () => {
      expect(monitor.hasError('network')).toBe(false);
    });

    it('should return true for category with errors', () => {
      monitor.processMessage('ECONNREFUSED');
      expect(monitor.hasError('network')).toBe(true);
    });
  });

  describe('getErrorsByCategory', () => {
    it('should return empty array when no errors', () => {
      expect(monitor.getErrorsByCategory('network')).toEqual([]);
    });

    it('should return errors for specific category', () => {
      monitor.processMessage('invalid api key');
      monitor.processMessage('rate limit exceeded');
      const authErrors = monitor.getErrorsByCategory('auth_invalid');
      expect(authErrors.length).toBe(1);
      expect(authErrors[0].category).toBe('auth_invalid');
    });
  });

  describe('reset', () => {
    it('should reset to healthy status', () => {
      monitor.processMessage('invalid api key');
      monitor.reset();
      expect(monitor.isHealthy()).toBe(true);
    });

    it('should clear error counts', () => {
      monitor.processMessage('invalid api key');
      monitor.reset();
      expect(monitor.getErrorCount('auth_invalid')).toBe(0);
    });

    it('should clear recent errors', () => {
      monitor.processMessage('invalid api key');
      monitor.reset();
      expect(monitor.getState().recentErrors).toEqual([]);
    });

    it('should clear last error', () => {
      monitor.processMessage('invalid api key');
      monitor.reset();
      expect(monitor.getLastError()).toBeUndefined();
    });
  });

  describe('setStatus', () => {
    it('should set status to degraded', () => {
      monitor.setStatus('degraded');
      expect(monitor.getState().status).toBe('degraded');
    });

    it('should set status to error', () => {
      monitor.setStatus('error');
      expect(monitor.getState().status).toBe('error');
    });

    it('should set status to healthy', () => {
      monitor.processMessage('invalid api key');
      monitor.setStatus('healthy');
      expect(monitor.isHealthy()).toBe(true);
    });

    it('should schedule recovery when set to degraded', () => {
      monitor.setStatus('degraded');
      vi.advanceTimersByTime(60_000);
      expect(monitor.isHealthy()).toBe(true);
    });
  });

  describe('getSummary', () => {
    it('should return "OK" when healthy', () => {
      expect(monitor.getSummary()).toBe('OK');
    });

    it('should return status and error info when not healthy', () => {
      monitor.processMessage('invalid api key');
      const summary = monitor.getSummary();
      expect(summary).toContain('ERROR');
      expect(summary).toContain('auth_invalid');
    });

    it('should truncate long messages', () => {
      const longMessage = 'A'.repeat(100);
      monitor.processMessage(`invalid api key ${longMessage}`);
      const summary = monitor.getSummary();
      expect(summary.length).toBeLessThan(150);
      expect(summary).toContain('...');
    });
  });

  describe('getState', () => {
    it('should return a copy of state', () => {
      const state1 = monitor.getState();
      const state2 = monitor.getState();
      expect(state1).not.toBe(state2);
      expect(state1.recentErrors).not.toBe(state2.recentErrors);
      expect(state1.errorCounts).not.toBe(state2.errorCounts);
    });
  });
});

describe('typed classification', () => {
  it('uses the error kind instead of matching text', () => {
    const monitor = new ExchangeHealthMonitor('bybit');
    expect(monitor.processMessage('something new happened', true, { kind: 'authInvalid' })).toBe('auth_invalid');
    expect(monitor.processMessage('invalid api key', true, { outcome: 'unknown' })).toBe('auth_invalid');
    expect(monitor.processMessage('unmatched', true, { kind: 'rateLimited' })).toBe('rate_limit');
    // A catch-all kind never hides a text-detected auth failure.
    expect(monitor.processMessage('invalid api key', true, { kind: 'rejected' })).toBe('auth_invalid');
    expect(monitor.processMessage('something unmatched', true, { kind: 'rejected' })).toBe('order_rejected');
  });

  it('reports an insufficient balance by its kind, even when the text matches another pattern', () => {
    const monitor = new ExchangeHealthMonitor('binance');
    expect(monitor.processMessage('Margin is insufficient.', true, { kind: 'insufficientBalance' })).toBe(
      'insufficient_balance',
    );
    expect(monitor.processMessage('rejected with 503', true, { kind: 'insufficientBalance' })).toBe(
      'insufficient_balance',
    );
    expect(monitor.getState().errorCounts.insufficient_balance).toBe(2);
  });
});


describe('coded native health', () => {
  it('uses the code despite changed or misleading auth wording', () => {
    const monitor = new ExchangeHealthMonitor('bybit');
    const event = (code: string, message: string) => ({ code, message, level: 'warn' as const, fields: {} });
    expect(monitor.processEvent(event('auth.rejected', 'new credential refusal'))).toBe('auth_invalid');
    expect(monitor.processEvent(event('request.failed', 'invalid api key'))).toBe('unknown');
    expect(monitor.processEvent(event('rate_limited', 'unauthorized'))).toBe('rate_limit');
    expect(monitor.getErrorCount('auth_invalid')).toBe(1);
    expect(monitor.getLastError()?.code).toBe('rate_limited');
    expect(monitor.getState().status).toBe('error');
    monitor.dispose();
  });

  it('reads the kind a classified notice carries in its fields', () => {
    const monitor = new ExchangeHealthMonitor('binance');
    const fields = { kind: 'insufficientBalance', venueCode: '-2019', venueMessage: 'Margin is insufficient.' };
    expect(monitor.processEvent({ code: 'order.rejected', message: 'Margin is insufficient.', level: 'warn', fields }))
      .toBe('insufficient_balance');
    expect(monitor.processEvent({ code: 'auth.rejected', message: 'x', level: 'error', fields: { kind: 'authPermission' } }))
      .toBe('auth_permissions');
    expect(monitor.processEvent({ code: 'request.failed', message: 'x', level: 'warn', fields: { kind: 'rateLimited' } }))
      .toBe('rate_limit');
    // An explicit classification still wins, and an unknown field kind is ignored.
    expect(monitor.processEvent({ code: 'order.rejected', message: 'x', level: 'warn', fields }, { kind: 'invalidInput' }))
      .toBe('order_rejected');
    expect(monitor.processEvent({ code: 'request.failed', message: 'x', level: 'warn', fields: { kind: 'made-up' } }))
      .toBe('unknown');
    monitor.dispose();
  });

  it('keeps typed auth subcategories and ignores routine diagnostic text', () => {
    const monitor = new ExchangeHealthMonitor('bybit');
    expect(monitor.processEvent({ code: 'auth.rejected', message: '', level: 'error', fields: {} },
      { kind: 'authExpired' })).toBe('auth_expired');
    expect(monitor.processEvent({ code: 'auth.rejected', message: 'invalid api key', level: 'error', fields: {} },
      { kind: 'authPermission' })).toBe('auth_permissions');
    monitor.reset();
    expect(monitor.processEvent({ code: 'startup.progress', message: 'invalid api key', level: 'info', fields: {} })).toBeNull();
    expect(monitor.processEvent({ code: 'ws.closed', message: 'closed', level: 'debug', fields: {} })).toBe('network');
    expect(monitor.isHealthy()).toBe(true);
    monitor.dispose();
  });

  it('copies authoritative counters without counting reports twice or inferring hidden messages', () => {
    const monitor = new ExchangeHealthMonitor('bybit');
    const health = { wsOpened: 3, wsClosed: 2, reconnects: 2, authFailures: 1, httpFailures: 4,
      rateLimited: 2, snapshotsDiscarded: 5, rowsSkipped: 7, lastPrivateMessage: 123 };
    monitor.processEvent({ code: 'auth.rejected', message: 'refused', level: 'error', fields: {} });
    monitor.processHealth(health);
    monitor.processHealth(health);
    health.rowsSkipped = 99;
    const state = monitor.getState();
    expect(state.engineHealth).toEqual({ ...health, rowsSkipped: 7 });
    state.engineHealth!.wsOpened = 99;
    expect(monitor.getState().engineHealth?.wsOpened).toBe(3);
    expect(monitor.getErrorCount('auth_invalid')).toBe(1);
    monitor.processHealth(undefined);
    expect(monitor.getState().engineHealth?.rowsSkipped).toBe(7);
    monitor.reset();
    expect(monitor.getState().engineHealth).toBeUndefined();
    monitor.dispose();
  });
});
