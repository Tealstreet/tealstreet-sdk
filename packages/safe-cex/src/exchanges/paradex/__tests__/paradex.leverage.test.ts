import { describe, it, expect } from 'vitest';
import axios from 'axios';

describe('Paradex Leverage Limits', () => {
  it('should calculate max leverage from IMF correctly for BTC-USD-PERP', async () => {
    // Fetch markets directly from Paradex API
    const response = await axios.get('https://api.prod.paradex.trade/v1/markets');
    const markets = response.data.results;

    // Find BTC-USD-PERP
    const btcMarket = markets.find((m: any) => m.symbol === 'BTC-USD-PERP');

    expect(btcMarket).toBeDefined();
    expect(btcMarket.delta1_cross_margin_params).toBeDefined();
    expect(btcMarket.delta1_cross_margin_params.imf_base).toBe('0.02');

    // Calculate max leverage using our formula
    const imf = parseFloat(btcMarket.delta1_cross_margin_params.imf_base);
    const maxLeverage = Math.floor(1 / imf);

    expect(maxLeverage).toBe(50);

    console.log(`✓ BTC-USD-PERP: IMF=${imf}, Max Leverage=${maxLeverage}x`);
  });

  it('should calculate leverage as 1/IMF for various IMF values', () => {
    // Test the formula directly
    const testCases = [
      { imf: 0.02, expectedLeverage: 50 },
      { imf: 0.01, expectedLeverage: 100 },
      { imf: 0.05, expectedLeverage: 20 },
      { imf: 0.1, expectedLeverage: 10 },
    ];

    testCases.forEach(({ imf, expectedLeverage }) => {
      const calculated = Math.floor(1 / imf);
      expect(calculated).toBe(expectedLeverage);
    });
  });
});
