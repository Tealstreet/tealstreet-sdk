import type { ExchangeOptions } from '../../types';

import { afterEach, describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../reference/hyperliquid/hyperliquid.exchange';
import { ENDPOINTS } from '../../exchanges/hyperliquid/hyperliquid.types';
import { DefaultStore } from '../../store/store.base';

const AGENT = '0x1111111111111111111111111111111111111111';
const MASTER = '0x2222222222222222222222222222222222222222';
const VAULT = '0x3333333333333333333333333333333333333333';
const DEFAULT_FEES = { takerFee: 0.00045, makerFee: 0.00015 };
const exchanges: HyperliquidExchange[] = [];
const setup = (overrides: Partial<ExchangeOptions> = {}) => {
  const exchange = new HyperliquidExchange(
    'hyperliquid',
    {
      key: AGENT,
      secret: '',
      applicationId: MASTER,
      tuning: { consoleLogging: false },
      ...overrides,
    },
    new DefaultStore(),
  );
  exchanges.push(exchange);
  const post = vi.spyOn((exchange as any).infoXhr, 'post').mockResolvedValue({
    data: { userCrossRate: '0.00032', userAddRate: '0.00012' },
  });
  return { exchange, post, fee: () => (exchange as any).fetchFee() };
};
afterEach(() => {
  for (const exchange of exchanges.splice(0)) exchange.dispose();
  vi.restoreAllMocks();
});

describe('Hyperliquid userFees trading-account subject', () => {
  it('queries the master account rather than the API-agent address', async () => {
    const { post, fee } = setup();
    await expect(fee()).resolves.toEqual({ takerFee: 0.00032, makerFee: 0.00012 });
    expect(post).toHaveBeenCalledExactlyOnceWith(
      ENDPOINTS.INFO,
      { type: 'userFees', user: MASTER },
      { headers: { 'Content-Type': 'application/json' } },
    );
  });

  it('prefers the vault over the configured master and API-agent addresses', async () => {
    const { post, fee } = setup({ vault: VAULT });
    await fee();
    expect(post).toHaveBeenCalledWith(ENDPOINTS.INFO, { type: 'userFees', user: VAULT }, expect.anything());
  });

  it('uses the identity discovered by the existing startup sequence', async () => {
    const { exchange, post, fee } = setup({ applicationId: undefined });
    post.mockImplementation(async (_url: unknown, body: any) => ({
      data:
        body.type === 'userRole'
          ? { role: 'agent', data: { user: MASTER } }
          : { userCrossRate: '0.00032', userAddRate: '0.00012' },
    }));
    vi.spyOn(exchange as any, 'initExternalPublicData').mockResolvedValue(false);
    vi.spyOn(exchange as any, 'resolveUserAbstractionMode').mockResolvedValue(undefined);
    vi.spyOn(exchange as any, 'fetchCorePerpsState').mockResolvedValue(null);
    vi.spyOn(exchange as any, 'fetchBuilderFeeApproval').mockResolvedValue(undefined);
    vi.spyOn(exchange as any, 'fetchMarketsAndTickers').mockImplementation(async () => {
      await fee();
      return [[], []];
    });
    vi.spyOn(exchange, 'refreshOrders').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'fetchAllHip3Positions').mockResolvedValue(undefined);
    vi.spyOn(exchange, 'tick').mockResolvedValue(undefined);
    vi.spyOn(exchange.publicWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    vi.spyOn(exchange.privateWebsocket, 'connectAndSubscribe').mockImplementation(() => {});
    await exchange.start();
    expect(post.mock.calls.map(([, body]) => body)).toEqual([
      { type: 'userRole', user: AGENT },
      { type: 'userFees', user: MASTER },
    ]);
  });

  it.each([
    { applicationId: undefined, publicDataOnly: false },
    { applicationId: undefined, key: '' },
    { publicDataOnly: true, vault: VAULT },
  ])('keeps default fees without I/O for unresolved/public-only input %j', async (options) => {
    const { post, fee } = setup(options);
    await expect(fee()).resolves.toEqual(DEFAULT_FEES);
    expect(post).not.toHaveBeenCalled();
  });

  it('retains the existing empty-response and request-failure defaults', async () => {
    const { post, fee } = setup();
    post.mockResolvedValueOnce({ data: null });
    await expect(fee()).resolves.toEqual(DEFAULT_FEES);
    post.mockRejectedValueOnce(new Error('unavailable'));
    await expect(fee()).resolves.toEqual(DEFAULT_FEES);
  });

  it('retains existing crossRate/addRate response aliases', async () => {
    const { post, fee } = setup();
    post.mockResolvedValueOnce({ data: { crossRate: '0.0004', addRate: '0.0001' } });
    await expect(fee()).resolves.toEqual({ takerFee: 0.0004, makerFee: 0.0001 });
  });
});
