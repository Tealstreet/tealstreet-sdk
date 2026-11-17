import type { ExchangeOptions } from '../../types';

import { describe, expect, it, vi } from 'vitest';

import { exchangeRegistry } from '../../exchanges/registry';
import * as betaPolicy from '../../exchanges/rust/betaVenues';
import { RustEngineExchange } from '../../exchanges/rust/rust.exchange';
import { createRustRegistry } from '../../exchanges/rustRegistry';
import { FrameStore } from '../../store/frameStore';
import { OrderStatus } from '../../types';
import { adoptEngineCapabilities } from '../fixtures/engineCapabilities';
import { TxFlowExchange } from '../reference/txflow/txflow.exchange';

vi.mock('../../exchanges/rust/betaVenues', async (importOriginal) => {
  const { retainedRustPolicy } = await import('../reference/retainedRustPolicy');
  return retainedRustPolicy(await importOriginal<typeof import('../../exchanges/rust/betaVenues')>());
});

const options: ExchangeOptions = {
  key: '',
  secret: '0x' + 'ab'.repeat(32),
  applicationId: '0x0000000000000000000000000000000000000001',
  testnet: true,
};
describe('TxFlow account identity at the worker seam', () => {
  it.each([{ orderId: '42' }, { status: [OrderStatus.Closed] }, { orderId: '42', status: [OrderStatus.Closed] }])(
    'rejects unsupported order-history filters before dispatch: %j',
    async (filters) => {
      const exchange = new RustEngineExchange('txflow', options, new FrameStore());
      adoptEngineCapabilities(exchange);
      const command = vi.spyOn(exchange as never as { command: (args: unknown) => Promise<unknown> }, 'command');
      try {
        await expect(exchange.fetchHistoricalOrders(filters)).rejects.toThrow(
          'TxFlow order history does not support orderId or status filters',
        );
        expect(command).not.toHaveBeenCalled();
      } finally {
        exchange.dispose();
      }
    },
  );
  it('refuses TxFlow in the production beta registry', async () => {
    // This admission case restores both real guards below the historical fixture boundary.
    const actual = await vi.importActual<typeof import('../../exchanges/rust/betaVenues')>(
      '../../exchanges/rust/betaVenues',
    );
    const venueGuard = vi.spyOn(betaPolicy, 'assertBetaVenue').mockImplementation(actual.assertBetaVenue);
    const accountGuard = vi.spyOn(betaPolicy, 'assertBetaAccount').mockImplementation(actual.assertBetaAccount);
    const load = vi.fn(async () => RustEngineExchange);
    try {
      await expect(exchangeRegistry.txflow()).rejects.toMatchObject({ kind: 'unsupported', outcome: 'notSent' });
      await expect(createRustRegistry(load).txflow()).rejects.toMatchObject({
        kind: 'unsupported',
        outcome: 'notSent',
      });
      expect(load).not.toHaveBeenCalled();
      expect(venueGuard).toHaveBeenCalledWith('txflow');
    } finally {
      venueGuard.mockRestore();
      accountGuard.mockRestore();
    }
  });
  it.each([TxFlowExchange, RustEngineExchange])('keeps agent-only credentials private without an API key', (Ctor) => {
    const exchange = new Ctor('txflow', options, new FrameStore());
    try {
      expect(exchange.isPublicDataOnly).toBe(false);
    } finally {
      exchange.dispose();
    }
  });
  it.each([TxFlowExchange, RustEngineExchange])(
    'honors explicit public mode without silently downgrading invalid private startup',
    (Ctor) => {
      const publicExchange = new Ctor('txflow', { ...options, publicDataOnly: true }, new FrameStore());
      const invalid = new Ctor('txflow', { ...options, secret: '' }, new FrameStore());
      try {
        expect(publicExchange.isPublicDataOnly).toBe(true);
        expect(invalid.isPublicDataOnly).toBe(false);
      } finally {
        publicExchange.dispose();
        invalid.dispose();
      }
    },
  );
  it('reads confirmed leverage rails instead of replacing them with the market maximum', async () => {
    const exchange = new RustEngineExchange('txflow', options, new FrameStore());
    const rails = { buyLeverage: 5, sellLeverage: 8, marginType: 'isolated' };
    const command = vi
      .spyOn(exchange as never as { command: (args: unknown) => Promise<unknown> }, 'command')
      .mockResolvedValue(rails);
    adoptEngineCapabilities(exchange);
    try {
      expect(await exchange.fetchLeverageForSymbol('BTC-USDC')).toEqual(rails);
      expect(command).toHaveBeenCalledWith({ op: 'fetchLeverage', symbol: 'BTC-USDC' }, { report: false });
    } finally {
      exchange.dispose();
    }
  });
});
