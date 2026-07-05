import { describe, expect, it, vi } from 'vitest';

import { HyperliquidExchange } from '../exchanges/hyperliquid/hyperliquid.exchange';
import { DefaultStore } from '../store/store.base';
import { ExchangeOptions } from '../types';

const TEST_PRIVATE_KEY = '0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80';
const TEST_ADDRESS = '0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266';
const TEST_MASTER = '0x1111111111111111111111111111111111111111';

const createExchange = (overrides: Partial<ExchangeOptions> = {}) => {
  const exchange = new HyperliquidExchange(
    'hyperliquid',
    {
      key: TEST_ADDRESS,
      secret: TEST_PRIVATE_KEY,
      applicationId: TEST_MASTER,
      testnet: true,
      tuning: { consoleLogging: false },
      ...overrides,
    },
    new DefaultStore(),
  );

  return exchange;
};

describe('HyperliquidExchange validateAccount', () => {
  it('rejects an API wallet Hyperliquid no longer recognises', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.infoXhr, 'post').mockResolvedValue({ data: { role: 'missing' } });

    const result = await exchange.validateAccount();

    expect(result.error).toContain('does not recognise API wallet');
  });

  it('rejects an agent wallet linked to a different main wallet', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.infoXhr, 'post').mockResolvedValue({
      data: {
        role: 'agent',
        data: { user: '0x2222222222222222222222222222222222222222' },
      },
    });

    const result = await exchange.validateAccount();

    expect(result.error).toContain(`not ${TEST_MASTER}`);
  });

  it('accepts an agent wallet linked to the configured main wallet', async () => {
    const exchange = createExchange();
    vi.spyOn(exchange.infoXhr, 'post').mockResolvedValue({
      data: {
        role: 'agent',
        data: { user: TEST_MASTER },
      },
    });

    const result = await exchange.validateAccount();

    expect(result.error).toBe('');
  });
});
