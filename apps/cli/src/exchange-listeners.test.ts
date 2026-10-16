import { EventEmitter } from 'node:events';

import { describe, expect, it, vi } from 'vitest';

import { OrderSide, type OrderFillEvent } from '../../../packages/safe-cex/src/types';
import { setupExchangeListeners, type ExchangeListenerContext } from './exchange-listeners';

describe('setupExchangeListeners', () => {
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
