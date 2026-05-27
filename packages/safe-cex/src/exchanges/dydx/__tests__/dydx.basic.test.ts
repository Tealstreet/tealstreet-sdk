import { describe, it, expect } from 'vitest';
import { DydxExchange } from '../dydx.exchange';
import { ExchangeOptions } from '../../../types';
import { DefaultStore } from '../../../store/store.base';
import { CandleResolution } from '../dydx.types';

describe('DydxExchange - Basic Tests', () => {
  it('should create dydx exchange instance', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    expect(exchange).toBeDefined();
    expect(exchange.options).toBeDefined();
    expect(exchange.options.testnet).toBe(true);
  });

  it('should generate unique client IDs', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    const clientId1 = (exchange as any).generateClientId();
    const clientId2 = (exchange as any).generateClientId();
    const clientId3 = (exchange as any).generateClientId();

    expect(clientId1).toBeDefined();
    expect(clientId2).toBeDefined();
    expect(clientId3).toBeDefined();

    // Client IDs should be unique
    expect(clientId1).not.toBe(clientId2);
    expect(clientId2).not.toBe(clientId3);
    expect(clientId1).not.toBe(clientId3);

    // Client IDs should be within uint32 range
    expect(clientId1).toBeGreaterThanOrEqual(0);
    expect(clientId1).toBeLessThanOrEqual(4294967295); // 2^32 - 1
    expect(clientId2).toBeGreaterThanOrEqual(0);
    expect(clientId2).toBeLessThanOrEqual(4294967295);
  });

  it('should convert intervals to dydx resolutions', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    expect((exchange as any).convertInterval('1m')).toBe(CandleResolution.ONE_MIN);
    expect((exchange as any).convertInterval('5m')).toBe(CandleResolution.FIVE_MINS);
    expect((exchange as any).convertInterval('15m')).toBe(CandleResolution.FIFTEEN_MINS);
    expect((exchange as any).convertInterval('30m')).toBe(CandleResolution.THIRTY_MINS);
    expect((exchange as any).convertInterval('1h')).toBe(CandleResolution.ONE_HOUR);
    expect((exchange as any).convertInterval('4h')).toBe(CandleResolution.FOUR_HOURS);
    expect((exchange as any).convertInterval('1d')).toBe(CandleResolution.ONE_DAY);
  });

  it('should convert dydx order side to safe-cex', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    expect((exchange as any).dydxOrderSideToSafeCex('BUY')).toBe('buy');
    expect((exchange as any).dydxOrderSideToSafeCex('SELL')).toBe('sell');
  });

  it('should convert safe-cex order side to dydx', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    const OrderSide = { Buy: 'buy', Sell: 'sell' };
    expect((exchange as any).safeCexOrderSideToDydx(OrderSide.Buy)).toBe('BUY');
    expect((exchange as any).safeCexOrderSideToDydx(OrderSide.Sell)).toBe('SELL');
  });

  it('should return one-way position mode', async () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    const positionMode = await exchange.getPositionMode();
    expect(positionMode).toBe('oneway');
  });

  it('should have testnet configuration when testnet option is true', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    expect(exchange.options.testnet).toBe(true);
  });

  it('should have mainnet configuration when testnet option is false', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: false,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    expect(exchange.options.testnet).toBe(false);
  });

  it('should initialize with default subaccount number 0', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    // Subaccount number should default to 0 if not provided
    expect(exchange.options.dydxSubaccountNumber || 0).toBe(0);
  });

  it('should use custom subaccount number if provided', () => {
    const options: ExchangeOptions = {
      key: 'test-wallet-address',
      secret: 'test-mnemonic',
      testnet: true,
      dydxSubaccountNumber: 5,
    };

    const store = new DefaultStore();
    const exchange = new DydxExchange('dydx', options, store);

    expect(exchange.options.dydxSubaccountNumber).toBe(5);
  });
});
