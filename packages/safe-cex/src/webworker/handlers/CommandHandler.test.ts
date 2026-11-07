import type { StoreWorkerPayload } from '../StoreWorkerPayload';

import { describe, expect, it } from 'vitest';

import { handlerArgs, NO_ALGOS, plainExchange, rustAlgoExchange } from '../../__tests__/fixtures/rustAlgoExchange';
import { CommandHandler } from './CommandHandler';

const exec = (data: unknown) => ({ op: 'exec', data }) as StoreWorkerPayload;

describe('CommandHandler', () => {
  it('sends exec to the account engine Rust algos', () => {
    const exchange = rustAlgoExchange();
    const data = { symbol: 'BTCUSDT', command: 'buy 0.01', reqId: 'r1' };
    new CommandHandler(...handlerArgs(exchange)).handle(exec(data));
    expect(exchange.routeAlgoOp).toHaveBeenCalledWith(
      'test-account',
      'exec',
      expect.any(Function),
      expect.any(Function),
    );
    expect(exchange.algoOp).toHaveBeenCalledWith('exec', data);
  });

  it('answers a malformed exec payload instead of throwing', () => {
    const exchange = rustAlgoExchange();
    expect(() => new CommandHandler(...handlerArgs(exchange)).handle(exec(undefined))).not.toThrow();
    expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-log', { message: 'Invalid command payload.' });
    expect(exchange.routeAlgoOp).not.toHaveBeenCalled();
  });

  it('answers a refused reqId with a failed exec-result', () => {
    const exchange = rustAlgoExchange('refuse');
    new CommandHandler(...handlerArgs(exchange)).handle(exec({ symbol: 'BTCUSDT', command: 'buy 1', reqId: 'r2' }));
    const message = `exec ${NO_ALGOS}`;
    expect(exchange.algoOp).not.toHaveBeenCalled();
    expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-log', { message, reqId: 'r2' });
    expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-result', {
      reqId: 'r2',
      handled: false,
      success: false,
      error: message,
    });
  });

  it('refuses exec on an account that is not a Rust engine', () => {
    const exchange = plainExchange();
    new CommandHandler(...handlerArgs(exchange)).handle(exec({ symbol: 'BTCUSDT', command: 'buy 1' }));
    expect(exchange.emitter.emit).toHaveBeenCalledWith('exec-log', {
      message: 'exec was not run: algos run only on a Rust engine account',
    });
    expect(exchange.emitter.emit).toHaveBeenCalledTimes(1);
  });

  it('ignores other ops', () => {
    const exchange = rustAlgoExchange();
    new CommandHandler(...handlerArgs(exchange)).handle({ op: 'chaser:start', data: {} } as StoreWorkerPayload);
    expect(exchange.routeAlgoOp).not.toHaveBeenCalled();
    expect(exchange.emitter.emit).not.toHaveBeenCalled();
  });
});
