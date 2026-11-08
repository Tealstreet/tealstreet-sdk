import type { ClosedPnlEntry, ExchangeName } from '../../types';
import type { EngineCapabilities } from '@tealstreet/safe-cex-wasm/host';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ExchangeError } from '@tealstreet/safe-cex-wasm/host';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import fixtures from '../../../../../sdk-rs/engine/tests/fixtures/history-closed-output.json';

const exchanges: RustEngineExchange[] = [];
afterEach(() => { for (const exchange of exchanges.splice(0)) exchange.dispose(); });

function make(name: ExchangeName, rows: unknown, options: Record<string, unknown> = {}, capabilities: Partial<EngineCapabilities> = {}) {
  const exchange = new RustEngineExchange(name, { key: 'k', secret: 's', passphrase: 'p', ...options }, new DefaultStore());
  exchanges.push(exchange);
  adoptEngineCapabilities(exchange, capabilities);
  const command = vi.fn(async (_command: Record<string, unknown>): Promise<unknown> => {
    if (rows instanceof Error) throw rows;
    return rows;
  });
  Object.defineProperty(exchange, 'command', { value: command });
  return { exchange, command };
}

describe('Rust reported closed-PnL output is authoritative', () => {
  it('forwards closed-PnL to the worker after capability adoption without a local command', async () => {
    const { exchange, command } = make('bybit', fixtures.bybit, { workerMirror: true }, { closedPositions: true });
    const calls: { method: string; args: unknown[] }[] = [];
    exchange.on('callMethod', (event: { requestId: string; method: string; args: unknown[] }) => {
      calls.push({ method: event.method, args: event.args });
      exchange.emitter.emit('callMethod:done', { requestId: event.requestId, success: true, result: fixtures.bybit });
    });
    const opts = { symbol: 'BTCUSD' };
    await exchange.fetchClosedPositions(opts);
    expect(calls).toEqual([{ method: 'fetchClosedPositions', args: [opts] }]);
    expect(command).not.toHaveBeenCalled();
    calls.length = 0;

    expect(await exchange.fetchClosedPnl(opts)).toEqual(fixtures.bybit);
    expect({ worker: calls.length, local: command.mock.calls.length }).toEqual({ worker: 1, local: 0 });
    expect(calls).toEqual([{ method: 'fetchClosedPnl', args: [opts] }]);
  });

  it.each<ExchangeName>(['bybit', 'okx', 'blofin', 'bitunix'])('%s forwards the reported command with account and bounds', async name => {
    const { exchange, command } = make(name, fixtures.bybit);
    expect(await exchange.fetchClosedPnl({symbol: 'BTCUSD', from: 1, to: 2, accountName: 'fixture-account'})).toBe(fixtures.bybit);
    expect(command).toHaveBeenCalledWith({op: 'fetchClosedPnl', symbol: 'BTCUSD', from: 1, to: 2, account: 'fixture-account'});
  });
  it('does not remap native side, quantities or fees according to the SDK venue name', async () => {
    const { exchange } = make('binance', fixtures.bybit, {}, {closedPositions: true, closedPositionFormat: 'bybit', inverseClosedPositions: false});
    const entries: ClosedPnlEntry[] = await exchange.fetchClosedPnl({symbol: 'BTCUSD'});
    expect(entries).toEqual(fixtures.bybit);
    expect(entries[0].contracts).toBe(0.01);
    expect(entries[0].side).toBe('long');
  });
  it.each<ExchangeName>(['okx','blofin'])('%s preserves account-wide empty-symbol normalization', async name => {
    const {exchange, command} = make(name, []);
    await exchange.fetchClosedPnl({symbol:''});
    expect(command).toHaveBeenCalledWith({op:'fetchClosedPnl',symbol:undefined,from:undefined,to:undefined,account:''});
  });
  it.each<ExchangeName>(['bybit','okx','blofin','bitunix'])('%s preserves incomplete empty output and typed failures', async name => {
    const rows = Object.defineProperty([], 'incomplete', {value:true});
    const {exchange} = make(name,rows);
    expect(await exchange.fetchClosedPnl({symbol:'BTCUSDT'})).toBe(rows);
    const error = new ExchangeError('Rate limit', {message:'Rate limit',kind:'rateLimited'});
    const failed = make(name,error).exchange;
    await expect(failed.fetchClosedPnl({symbol:'BTCUSDT'})).rejects.toBe(error);
  });
  it('public-only sessions issue no closed-PnL command',async()=>{
    const {exchange,command}=make('bybit',fixtures.bybit,{publicDataOnly:true});
    expect(await exchange.fetchClosedPnl({symbol:'BTCUSD'})).toEqual([]);
    expect(command).not.toHaveBeenCalled();
  });
});
