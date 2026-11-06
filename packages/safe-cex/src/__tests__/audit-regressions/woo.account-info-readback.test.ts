import { afterEach, describe, expect, it, vi } from 'vitest';

import { WOOXExchange } from '../reference/woo/woo.exchange';
import { ENDPOINTS } from '../reference/woo/woo.types';
import { DefaultStore } from '../../store/store.base';
import { PositionMode } from '../../types';

const accountInfo = {
  applicationId: 'safe-cex',
  account: 'account-id',
  alias: 'Main account',
  accountMode: 'FUTURES',
  positionMode: 'HEDGE_MODE',
  leverage: 10,
  takerFeeRate: 0.0005,
  makerFeeRate: 0.0002,
  interestRate: 0,
  futuresTakerFeeRate: 0.0005,
  futuresMakerFeeRate: 0.0002,
  otpauth: true,
  marginRatio: 0.125,
  openMarginRatio: 0.1,
  initialMarginRatio: 0.08,
  maintenanceMarginRatio: 0.04,
  totalCollateral: 1200.75,
  freeCollateral: 875.25,
  totalAccountValue: 1300.5,
  totalVaultValue: 100,
  totalStakingValue: 80,
  referrerID: 'WOO2023',
  accountType: 'Main',
  totalLaunchpadVaultValue: 50,
  totalEarnValue: 70,
};

const makeExchange = () => {
  const exchange = new WOOXExchange(
    'woo' as any,
    {
      key: 'key',
      secret: 'secret',
      applicationId: 'application-id',
      tuning: { consoleLogging: false },
    } as any,
    new DefaultStore(),
  );
  const get = vi.fn();
  exchange.xhr = { get } as any;
  return { exchange, get };
};

let exchange: WOOXExchange | undefined;

afterEach(() => {
  exchange?.dispose();
  exchange = undefined;
  vi.restoreAllMocks();
});

describe('WOO account info readback', () => {
  it('maps and caches the documented account position mode', async () => {
    const setup = makeExchange();
    exchange = setup.exchange;
    setup.get.mockResolvedValueOnce({ data: { success: true, data: accountInfo, timestamp: 1_725_000_000_000 } });

    await expect(exchange.getPositionMode()).resolves.toBe(PositionMode.Hedged);
    await expect(exchange.getPositionMode()).resolves.toBe(PositionMode.Hedged);

    expect(setup.get).toHaveBeenCalledTimes(1);
    expect(setup.get).toHaveBeenCalledWith(ENDPOINTS.ACCOUNT);
  });

  it('surfaces failed account envelopes instead of reading positionMode from missing data', async () => {
    const setup = makeExchange();
    exchange = setup.exchange;
    setup.get.mockResolvedValueOnce({
      data: {
        success: false,
        message: 'Invalid nonce. Please check your system time is synchronized.',
        timestamp: 1_725_000_000_000,
      },
    });

    await expect(exchange.getPositionMode()).rejects.toThrow(
      'Invalid nonce. Please check your system time is synchronized.',
    );
  });
});
