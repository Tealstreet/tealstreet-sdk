import { describe, expect, it } from 'vitest';

import { BASE_WS_URL, DEMO_WS_URL } from '../../exchanges/bybit/bybit.types';
import {
  BASE_WS_URL as SPOT_BASE_WS_URL,
  DEMO_WS_URL as SPOT_DEMO_WS_URL,
} from '../reference/bybit/bybitspot.types';

describe('Bybit demo WebSocket URL audit regression', () => {
  it('keeps demo-only URLs private and public market data on mainnet', () => {
    expect(DEMO_WS_URL).toEqual({
      private: {
        livenet: 'wss://stream-demo.bybit.com/v5/private',
        testnet: 'wss://stream-testnet.bybit.com/v5/private',
      },
    });
    expect(SPOT_DEMO_WS_URL).toEqual(DEMO_WS_URL);

    expect(BASE_WS_URL.public.linear.livenet).toBe('wss://stream.bybit.com/v5/public/linear');
    expect(BASE_WS_URL.public.inverse.livenet).toBe('wss://stream.bybit.com/v5/public/inverse');
    expect(SPOT_BASE_WS_URL.public.spot.livenet).toBe('wss://stream.bybit.com/v5/public/spot');
  });
});
