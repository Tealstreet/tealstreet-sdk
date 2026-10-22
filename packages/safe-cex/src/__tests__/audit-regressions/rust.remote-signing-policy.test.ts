import { describe, expect, it } from 'vitest';
import { deriveBridgeConstruction, deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import type { ExchangeName, ExchangeOptions } from '../../types';

describe('shared delegated signing declarations', () => {
  const binance: ExchangeName[] = ['binance', 'binancepm', 'binanceinverse', 'binancespot'];
  const bybit: ExchangeName[] = ['bybit', 'bybitinverse', 'bybitspot', 'bybitdemo', 'hyrotrader'];
  it.each([...binance, ...bybit])('%s declares one REST-only PayloadSigner policy', (name) => {
    const policy = deriveRemoteSigningPolicy(name);
    expect(policy?.payloadRestOnly).toBe(true);
    expect(policy?.protectedHeaders).toEqual(binance.includes(name) ? [] : ['X-BAPI-SIGN', 'X-BAPI-API-KEY', 'X-BAPI-TIMESTAMP']);
  });
  it('declarations preserve closed admission: only the landed Binance family is enabled', () => {
    for (const name of binance) expect(deriveBridgeConstruction(name, {} as ExchangeOptions).dispatchSigning).toBe(true);
    for (const name of bybit) expect(deriveBridgeConstruction(name, {} as ExchangeOptions).dispatchSigning).toBe(false);
    for (const name of ['okx', 'mexc', 'kraken'] as ExchangeName[]) expect(deriveRemoteSigningPolicy(name)).toBeUndefined();
  });
});
