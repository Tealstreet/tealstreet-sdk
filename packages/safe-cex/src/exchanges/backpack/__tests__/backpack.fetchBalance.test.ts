import { describe, it, expect, vi, afterEach } from 'vitest';
import { BackpackExchange } from '../backpack.exchange';
import { DefaultStore } from '../../../store/store.base';
import { BackpackCollateral } from '../backpack.types';

const createExchange = () =>
  new BackpackExchange(
    'backpack',
    {
      key: 'test-key',
      secret: 'test-secret',
      testnet: true,
    },
    new DefaultStore(),
  );

// Helper to create mock collateral response
const createMockCollateral = (overrides: Partial<BackpackCollateral> = {}): BackpackCollateral => ({
  assetsValue: '100',
  borrowLiability: '0',
  collateral: [],
  imf: '0.02',
  liabilitiesValue: '0',
  marginFraction: '10',
  mmf: '0.01',
  netEquity: '100',
  netEquityAvailable: '90',
  netEquityLocked: '10',
  netExposureFutures: '0',
  pnlUnrealized: '0',
  unsettledEquity: '0',
  ...overrides,
});

describe('BackpackExchange.fetchBalance', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('parses collateral response correctly', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: createMockCollateral({
        netEquity: '100.50',
        netEquityAvailable: '80.25',
        netEquityLocked: '20.25',
        pnlUnrealized: '-5.00',
        collateral: [
          {
            symbol: 'USDC',
            assetMarkPrice: '1',
            availableQuantity: '50',
            balanceNotional: '100',
            collateralValue: '100',
            collateralWeight: '1',
            lendQuantity: '50',
            openOrderQuantity: '0',
            totalQuantity: '100',
          },
        ],
      }),
    } as any);

    const balance = await exchange.fetchBalance();

    expect(balance.total).toBeCloseTo(100.5);
    expect(balance.free).toBeCloseTo(80.25);
    expect(balance.used).toBeCloseTo(20.25);
    expect(balance.upnl).toBeCloseTo(-5);
    expect(balance.currencies?.USDC).toMatchObject({
      available: 50,
      locked: 50,
      collateralValue: 100,
      used: 50,
      total: 100,
    });
  });

  it('handles multiple collateral assets', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: createMockCollateral({
        netEquity: '200',
        netEquityAvailable: '150',
        netEquityLocked: '50',
        collateral: [
          {
            symbol: 'USDC',
            assetMarkPrice: '1',
            availableQuantity: '50',
            balanceNotional: '100',
            collateralValue: '100',
            collateralWeight: '1',
            lendQuantity: '50',
            openOrderQuantity: '0',
            totalQuantity: '100',
          },
          {
            symbol: 'BTC',
            assetMarkPrice: '50000',
            availableQuantity: '0.001',
            balanceNotional: '50',
            collateralValue: '47.5',
            collateralWeight: '0.95',
            lendQuantity: '0.0005',
            openOrderQuantity: '0',
            totalQuantity: '0.0015',
          },
        ],
      }),
    } as any);

    const balance = await exchange.fetchBalance();
    const currencies = balance.currencies as Record<string, any>;

    expect(balance.total).toBeCloseTo(200);
    expect(currencies?.USDC?.total).toBeCloseTo(100);
    expect(currencies?.BTC?.total).toBeCloseTo(0.0015);
    expect(currencies?.BTC?.available).toBeCloseTo(0.001);
  });

  it('handles empty collateral array', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.xhr, 'get').mockResolvedValue({
      data: createMockCollateral({
        netEquity: '0',
        netEquityAvailable: '0',
        netEquityLocked: '0',
        collateral: [],
      }),
    } as any);

    const balance = await exchange.fetchBalance();

    expect(balance.total).toBe(0);
    expect(balance.free).toBe(0);
    expect(balance.used).toBe(0);
    expect(Object.keys(balance.currencies || {})).toHaveLength(0);
  });
});
