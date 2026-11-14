import { EventEmitter } from 'node:events';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { ExchangeHealthMonitor } from '../../../packages/safe-cex/src/health/ExchangeHealthMonitor';
import { OrderSide, type OrderFillEvent } from '../../../packages/safe-cex/src/types';
import { setupExchangeListeners, type ExchangeListenerContext } from './exchange-listeners';

afterEach(() => vi.restoreAllMocks());

const listenerContext = (): ExchangeListenerContext => ({
  adapter: { printSuccess: vi.fn(), printError: vi.fn() },
  logger: { info: vi.fn() },
  recordFill: vi.fn(),
  addWatchMessage: vi.fn(),
  isMonitorMode: () => false,
  getCurrentAccountName: () => undefined,
  getHealthMonitor: () => undefined,
});

describe('setupExchangeListeners', () => {
  it('handles command output, errors, warnings and audit fills once across wrappers of one emitter', () => {
    const emitter = new EventEmitter();
    const context = listenerContext();
    const log = vi.spyOn(console, 'log').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
    setupExchangeListeners({ emitter }, context, 'account-a');
    setupExchangeListeners({ emitter }, context, 'account-a');

    emitter.emit('exec-log', { message: 'command output' });
    emitter.emit('error', 'native failure');
    emitter.emit('warning', 'native warning');
    emitter.emit('fill', { id: 'fill-1', orderId: 'order-1', side: OrderSide.Buy, amount: 0.5, symbol: 'BTCUSDT', price: 100 });

    expect(log.mock.calls).toEqual([['command output']]);
    expect(error.mock.calls).toEqual([['Exchange error: native failure']]);
    expect(warning.mock.calls).toEqual([['Exchange warning: native warning']]);
    expect(context.recordFill).toHaveBeenCalledExactlyOnceWith('order-1', 'Bought 0.5 BTCUSDT @ $100');
    expect(context.adapter.printSuccess).toHaveBeenCalledTimes(1);
  });

  it('reads watch mode at event time and buffers each event once', () => {
    const emitter = new EventEmitter();
    const context = listenerContext();
    let watching = false;
    context.isMonitorMode = () => watching;
    setupExchangeListeners({ emitter }, context, 'watch-account');
    watching = true;
    setupExchangeListeners({ emitter }, context, 'watch-account');

    emitter.emit('exec-log', { message: 'command output' });
    emitter.emit('warning', 'native warning');
    emitter.emit('error', 'native failure');
    emitter.emit('fill', { id: 'fill-1', side: OrderSide.Sell, amount: 0.5, symbol: 'BTCUSDT', price: 100 });

    expect(vi.mocked(context.addWatchMessage).mock.calls).toEqual([
      ['info', 'command output'], ['warning', 'native warning'], ['error', 'native failure'],
      ['fill', 'Sold 0.5 BTCUSDT @ $100'],
    ]);
    expect(context.adapter.printSuccess).not.toHaveBeenCalled();
    expect(context.recordFill).toHaveBeenCalledTimes(1);
  });

  it.each([false, true])('preserves typed refusal classification over 503 text (watch=%s)', (watching) => {
    const emitter = new EventEmitter();
    const context = listenerContext();
    const monitor = new ExchangeHealthMonitor('bitget');
    const processMessage = vi.spyOn(monitor, 'processMessage');
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const classified = { kind: 'insufficientBalance', outcome: 'rejected', venueCode: '43012' };
    context.isMonitorMode = () => watching;
    context.getHealthMonitor = () => monitor;
    setupExchangeListeners({ emitter }, context, 'typed-account');
    try {
      emitter.emit('error', 'rejected with 503', { classified });

      expect(processMessage).toHaveBeenCalledExactlyOnceWith('rejected with 503', true, classified);
      expect(monitor.getState().lastError?.category).toBe('insufficient_balance');
      expect(warning).not.toHaveBeenCalled();
      if (watching) {
        expect(context.addWatchMessage).toHaveBeenCalledExactlyOnceWith('error', 'rejected with 503');
        expect(error).not.toHaveBeenCalled();
      } else {
        expect(error).toHaveBeenCalledExactlyOnceWith('Exchange error: rejected with 503');
      }
    } finally {
      monitor.dispose();
    }
  });

  it('preserves Unknown rate-limit guidance in warning output', () => {
    const emitter = new EventEmitter();
    const context = listenerContext();
    const monitor = new ExchangeHealthMonitor('bitget');
    const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    context.getHealthMonitor = () => monitor;
    setupExchangeListeners({ emitter }, context, 'unknown-account');
    const message = 'Order outcome is unknown. Check Open Orders before trying again.';
    try {
      emitter.emit('error', message, { classified: { kind: 'rateLimited', outcome: 'unknown' } });

      expect(warning).toHaveBeenCalledExactlyOnceWith(`Exchange warning: ${message}`);
      expect(error).not.toHaveBeenCalled();
      expect(monitor.getState().lastError?.category).toBe('rate_limit');
    } finally {
      monitor.dispose();
    }
  });

  it('prints each fill once after switching A → B → A', () => {
    const exchangeA = { emitter: new EventEmitter() };
    const exchangeB = { emitter: new EventEmitter() };
    const printSuccess = vi.fn();
    const context: ExchangeListenerContext = {
      adapter: { printSuccess, printError: vi.fn() },
      logger: { info: vi.fn() },
      recordFill: vi.fn(),
      addWatchMessage: vi.fn(),
      isMonitorMode: () => false,
      getCurrentAccountName: () => undefined,
      getHealthMonitor: () => undefined,
    };

    setupExchangeListeners(exchangeA, context, 'account-a');
    setupExchangeListeners(exchangeB, context, 'account-b');
    setupExchangeListeners(exchangeA, context, 'account-a');
    const fill: OrderFillEvent = { id: 'fill-1', side: OrderSide.Buy, amount: 0.5, symbol: 'BTCUSDT', price: 100 };
    exchangeA.emitter.emit('fill', fill);
    exchangeB.emitter.emit('fill', { ...fill, side: OrderSide.Sell });

    expect(printSuccess.mock.calls).toEqual([
      ['✓ Fill: Bought 0.5 BTCUSDT @ $100'],
      ['✓ Fill: Sold 0.5 BTCUSDT @ $100'],
    ]);
  });
});
