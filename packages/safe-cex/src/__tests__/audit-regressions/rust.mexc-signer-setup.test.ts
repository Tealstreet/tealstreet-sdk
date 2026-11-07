import { describe, expect, it } from 'vitest';
import type { ExchangeOptions } from '../../types';
import { deriveBridgeConstruction, deriveSignerSetupPolicy, deriveSignerSetupMapping } from '../../exchanges/rust/construction';
import { LocalSignerSetup, rustTrim, withoutSigningCredentials } from '../../exchanges/rust/localSignerSetup';
import { mapMEXCSignerSetup } from '../../exchanges/rust/mexcSignerSetup';
import type { WasmEngineLike } from '@tealstreet/safe-cex-wasm/host';

const vectors = [
  ['replay-key', 'mexc:a7c5c5f5cbc33ea500a4850d'],
  [' replay-key ', 'mexc:ae0c5396072d0d2a609af843'],
  ['\u0085replay-key\u0085', 'mexc:7650b0f09e155dbcc2aa6016'],
  ['\ufeffreplay-key\ufeff', 'mexc:cdf888fd46e388a15151113d'],
] as const;
const options = { key: 'replay-key', secret: 'secret', secondary_key: ' read ', secondary_secret: ' read-secret ', publicDataOnly: false } as ExchangeOptions;

describe('MEXC trusted persisted identity', () => {
  it.each(vectors)('preserves raw namespace before trimming %j', (key, stableId) => {
    const input = { ...options, key };
    const mapped = mapMEXCSignerSetup(input);
    expect(mapped.localSigner?.primary?.stableId).toBe(stableId);
    expect(input.localSigner).toBeUndefined();
    const setup = new LocalSignerSetup(mapped, deriveSignerSetupPolicy('mexc'));
    let capsule = '';
    let engineOptions = '';
    class Engine { constructor(_venue: string, json: string, credentials?: string) { capsule = credentials ?? ''; engineOptions = json; } }
    setup.construct(Engine as unknown as new (venue: string, options: string, credentials?: string) => WasmEngineLike, 'mexc', JSON.stringify(withoutSigningCredentials(mapped)));
    expect(JSON.parse(capsule).primary).toEqual({ key, secret: 'secret', stableId, trimCredential: true });
    expect(JSON.parse(capsule).secondary).toEqual({ key: ' read ', secret: ' read-secret ', trimCredential: false });
    expect(engineOptions).not.toContain('replay-key');
    expect(JSON.parse(engineOptions)).not.toHaveProperty('localSigner');
  });
  it('retains Rust whitespace normalization and BOM distinction', () => {
    expect(rustTrim('\u0085replay-key\u0085')).toBe('replay-key');
    expect(rustTrim('\ufeffreplay-key\ufeff')).toBe('\ufeffreplay-key\ufeff');
  });
  it('accepts exact explicit metadata and refuses conflicting or invalid metadata', () => {
    const stableId = vectors[0][1];
    expect(mapMEXCSignerSetup({ ...options, localSigner: { primary: { stableId } } }).localSigner?.primary?.stableId).toBe(stableId);
    for (const stableId of ['other-account', '', 'bad\naccount']) {
      expect(() => mapMEXCSignerSetup({ ...options, localSigner: { primary: { stableId } } })).toThrow();
    }
  });
  it('never takes the secondary mapping as the primary identity', () => {
    expect(mapMEXCSignerSetup({ ...options, localSigner: { secondary: { stableId: 'secondary' } } }).localSigner?.primary?.stableId).toBe(vectors[0][1]);
    expect(mapMEXCSignerSetup({ ...options, key: undefined } as unknown as ExchangeOptions).localSigner?.primary).toBeUndefined();
  });
  it('pins the real bridge and optional Primary signing declaration', () => {
    expect(deriveBridgeConstruction('mexc', options).dispatchSigning).toBe(true);
    expect(deriveSignerSetupMapping('mexc')).toBe('mexc');
    expect(deriveSignerSetupPolicy('mexc').primary?.signing).toBe('optional');
    expect(deriveSignerSetupPolicy('mexc').primary?.trimCredential).toBe(true);
    expect(deriveSignerSetupPolicy('mexc').secondary?.trimCredential).toBe(false);
  });
});
