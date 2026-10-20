import { describe, expect, it, vi } from 'vitest';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { exchangeRegistry } from '../../exchanges/registry';
import { TxFlowExchange } from '../reference/txflow/txflow.exchange';
import { DefaultStore } from '../../store/store.base';
import type { ExchangeOptions } from '../../types';
import { OrderStatus } from '../../types';

const options: ExchangeOptions = { key: '', secret: '0x' + 'ab'.repeat(32), applicationId: '0x0000000000000000000000000000000000000001', testnet: true };
describe('TxFlow account identity at the worker seam', () => {
  it.each([{ orderId: '42' }, { status: [OrderStatus.Closed] }, { orderId: '42', status: [OrderStatus.Closed] }])('rejects unsupported order-history filters before dispatch: %j', async (filters) => {
    const exchange = new RustEngineExchange('txflow', options, new DefaultStore());
    const command = vi.spyOn(exchange as never as { command: (args: unknown) => Promise<unknown> }, 'command');
    try {
      await expect(exchange.fetchHistoricalOrders(filters)).rejects.toThrow('TxFlow order history does not support orderId or status filters');
      expect(command).not.toHaveBeenCalled();
    } finally { exchange.dispose(); }
  });
  it('loads Rust from the production registry', async () => {
    expect(await exchangeRegistry.txflow()).toBe(RustEngineExchange);
  });
  it.each([TxFlowExchange, RustEngineExchange])('keeps agent-only credentials private without an API key', (Ctor) => {
    const exchange = new Ctor('txflow', options, new DefaultStore());
    try { expect(exchange.isPublicDataOnly).toBe(false); } finally { exchange.dispose(); }
  });
  it.each([TxFlowExchange, RustEngineExchange])('honors explicit public mode without silently downgrading invalid private startup', (Ctor) => {
    const publicExchange = new Ctor('txflow', { ...options, publicDataOnly: true }, new DefaultStore());
    const invalid = new Ctor('txflow', { ...options, secret: '' }, new DefaultStore());
    try { expect(publicExchange.isPublicDataOnly).toBe(true); expect(invalid.isPublicDataOnly).toBe(false); }
    finally { publicExchange.dispose(); invalid.dispose(); }
  });
  it('reads confirmed leverage rails instead of replacing them with the market maximum', async () => {
    const exchange = new RustEngineExchange('txflow', options, new DefaultStore());
    const rails = { buyLeverage: 5, sellLeverage: 8, marginType: 'isolated' };
    const command = vi.spyOn(exchange as never as { command: (args: unknown) => Promise<unknown> }, 'command').mockResolvedValue(rails);
    try {
      expect(await exchange.fetchLeverageForSymbol('BTC-USDC')).toEqual(rails);
      expect(command).toHaveBeenCalledWith({ op: 'fetchLeverage', symbol: 'BTC-USDC' });
    } finally { exchange.dispose(); }
  });
});
