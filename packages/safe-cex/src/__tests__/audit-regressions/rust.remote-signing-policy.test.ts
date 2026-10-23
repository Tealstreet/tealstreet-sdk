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
  it.each([...binance, ...bybit])('%s distinguishes reviewed runtime admission from future codec policy', (name) => {
    expect(deriveRemoteSigningPolicy(name)).toMatchObject({
      enabled: true,
      payloadSupported: name !== 'hyrotrader',
    });
  });
  it('declarations retain the landed dispatch-signing flags without enabling other families', () => {
    for (const name of binance) expect(deriveBridgeConstruction(name, {} as ExchangeOptions).dispatchSigning).toBe(true);
    for (const name of bybit) expect(deriveBridgeConstruction(name, {} as ExchangeOptions).dispatchSigning).toBe(true);
    for (const name of ['mexc', 'kraken'] as ExchangeName[]) expect(deriveRemoteSigningPolicy(name)).toBeUndefined();
  });
  it.each(['bitget', 'bitgetinverse', 'bitgetusdc'] as const)('%s admits RemoteSigner on the shared bitget wire while refusing unsupported private PayloadSigner', (name) => {
    expect(deriveRemoteSigningPolicy(name)).toEqual({ enabled: true, payloadSupported: false,
      protectedHeaders: ['ACCESS-KEY', 'ACCESS-SIGN', 'ACCESS-TIMESTAMP', 'ACCESS-PASSPHRASE'],
      wire: { exchangeName: 'bitget', extraData: 'secondsTimestamp', secondsSuffix: { method: 'GET', path: '/user/verify' } },
      signatureEncoding: 'base64', passphraseHeader: 'ACCESS-PASSPHRASE' });
    expect(deriveBridgeConstruction(name, {} as ExchangeOptions).dispatchSigning).toBe(true);
  });

  it.each(['okx', 'okxeu', 'okxdemo', 'okxeudemo'] as const)('%s admits RemoteSigner while refusing unsupported private PayloadSigner', (name) => {
    expect(deriveRemoteSigningPolicy(name)).toMatchObject({ enabled: true, payloadSupported: false,
      signatureEncoding: 'base64', passphraseHeader: 'OK-ACCESS-PASSPHRASE', wire: { exchangeName: name, extraData: 'secondsTimestamp', secondsSuffix: { method: 'GET', path: '/users/self/verify' } } });
    expect(deriveBridgeConstruction(name, {} as ExchangeOptions).dispatchSigning).toBe(true);
  });
});
