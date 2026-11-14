import { expect, it } from 'vitest';

import { DefaultStore } from '@tealstreet/safe-cex/src/store/store.base';
import { OrderSide, OrderStatus, OrderType, type Market, type Order } from '@tealstreet/safe-cex/src/types';
import type { ChaserOrderInstance } from '@tealstreet/safe-cex/src/webworker/chaserOrder/ChaserOrderInstance';
import type { ExchangeLike } from '../../types';
import { formatChaserStatus } from './ChaseCommand';

it.each([
  { name: 'partially filled DOGE contracts', step: 0.01, contractSize: 1000, amount: 1, remaining: 250, expected: '250' },
  { name: 'tiny spot BTC', step: 0.000001, contractSize: 1, amount: 0.000006, remaining: 0.000002, expected: '0.000002' },
  { name: 'inverse BTC', step: 1, contractSize: 1, amount: 1, remaining: 0.00001, expected: '0.00001', isInverse: true },
])('shows the current order remaining base size for $name', (size) => {
  const market = {
    symbol: 'TEST', precision: { amount: size.step, price: 0.1 },
    contractSize: size.contractSize, isInverse: size.isInverse,
  } as Market;
  const store = new DefaultStore();
  store.update({ markets: [market], orders: [{
    id: 'order-1', symbol: market.symbol, status: OrderStatus.Open, type: OrderType.Limit,
    amount: size.amount, remaining: size.remaining, price: 100,
  } as Order] });
  const instance = {
    options: { symbol: market.symbol, side: OrderSide.Buy, size: size.amount },
    state: { attempts: 1, stopReason: '' }, remainingSize: size.amount,
    currentOrderIds: ['order-1'], isStopped: false,
  } as unknown as ChaserOrderInstance;

  const output = formatChaserStatus(instance, { store } as ExchangeLike).replace(/\x1b\[[0-9;]*m/g, '');
  const orderLine = output.split('\n').find((line) => line.includes('Order:'));

  expect(orderLine?.split(' @ ')[1]).toBe(size.expected);
});
