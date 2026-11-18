import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { describe, expect, it } from 'vitest';

import { handlerArgs, NO_ALGOS, plainExchange, rustAlgoExchange } from '../../__tests__/fixtures/rustAlgoExchange';
import { ChaserOrderHandler } from './ChaserOrderHandler';

const start = { op: 'chaser:start', data: { symbol: 'BTCUSDT', size: 1, side: 'buy' } } as StoreWorkerPayload;

describe('ChaserOrderHandler', () => {
  it.each([start, { op: 'chaser:terminate', data: 'chase-1' }, { op: 'chaser:terminateAll' }] as StoreWorkerPayload[])(
    'sends $op to the account engine Rust algos',
    (payload) => {
      const exchange = rustAlgoExchange();
      new ChaserOrderHandler(...handlerArgs(exchange)).handle(payload);
      expect(exchange.routeAlgoOp).toHaveBeenCalledWith(
        'test-account',
        payload.op,
        expect.any(Function),
        expect.any(Function),
      );
      expect(exchange.algoOp).toHaveBeenCalledWith(payload.op, payload.data);
      expect(exchange.emitter.emit).not.toHaveBeenCalled();
    },
  );

  it('refuses a chase visibly on an engine build without algos', () => {
    const exchange = rustAlgoExchange('refuse');
    new ChaserOrderHandler(...handlerArgs(exchange)).handle(start);
    expect(exchange.algoOp).not.toHaveBeenCalled();
    expect(exchange.emitter.emit).toHaveBeenCalledWith('error', `chaser:start ${NO_ALGOS}`);
  });

  it('refuses a chase visibly on an account that is not a Rust engine', () => {
    const exchange = plainExchange();
    new ChaserOrderHandler(...handlerArgs(exchange)).handle(start);
    expect(exchange.emitter.emit).toHaveBeenCalledWith(
      'error',
      'chaser:start was not run: algos run only on a Rust engine account',
    );
  });

  it('ignores other ops', () => {
    const exchange = rustAlgoExchange();
    new ChaserOrderHandler(...handlerArgs(exchange)).handle({ op: 'twap:start', data: {} } as StoreWorkerPayload);
    expect(exchange.routeAlgoOp).not.toHaveBeenCalled();
  });
});
