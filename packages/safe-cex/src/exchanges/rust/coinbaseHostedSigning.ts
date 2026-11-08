import type { EngineHost } from '@tealstreet/safe-cex-wasm/host';

import { uuid } from '../../utils/uuid';

const encode = (value: unknown) =>
  btoa(Array.from(new TextEncoder().encode(JSON.stringify(value)), (c) => String.fromCharCode(c)).join(''))
    .replace(/=+$/, '')
    .replace(/\+/g, '-')
    .replace(/\//g, '_');

/** Only public JWT fields are rendered here; the device's provider owns the key. */
export const signCoinbaseJwt = async (
  host: Pick<EngineHost, 'hostedSigning'> & Partial<Pick<EngineHost, 'coinbaseSigningMetadata' | 'hostedSigningClock'>>,
  uri?: string,
) => {
  if (!host.coinbaseSigningMetadata || !host.hostedSigningClock)
    throw new Error('Coinbase signing metadata is unavailable');
  const { keyName, algorithm } = host.coinbaseSigningMetadata();
  const { stampMs } = host.hostedSigningClock();
  const nbf = Math.floor(stampMs / 1000);
  if (!Number.isSafeInteger(nbf) || nbf < 0 || !['es256', 'edDsa'].includes(algorithm))
    throw new Error('Coinbase signing metadata is unavailable');
  const header = { alg: algorithm === 'es256' ? 'ES256' : 'EdDSA', typ: 'JWT', kid: keyName, nonce: uuid() };
  const claims = {
    iss: algorithm === 'es256' ? 'coinbase-cloud' : 'cdp',
    ...(algorithm === 'edDsa' && { aud: ['cdp_service'] }),
    nbf,
    exp: nbf + 120,
    sub: keyName,
    ...(uri && { uri }),
  };
  const toSign = `${encode(header)}.${encode(claims)}`;
  const output = await host.hostedSigning({ kind: 'wsAuth', toSign, outputFormat: 'coinbase-jws' });
  if (!output.signature || !/^[A-Za-z0-9_-]{86}$/.test(output.signature))
    throw new Error('Coinbase signing result is unavailable');
  return `${toSign}.${output.signature}`;
};

/** Validate the complete canonical public statement before invoking a delegate. */
export const inspectCoinbaseJws = (input: string, key: string, algorithm: 'es256' | 'ed25519'): { uri?: string } => {
  const invalid = (): never => { throw new Error('Coinbase signing request refused'); };
  const parts = input.split('.');
  if (parts.length !== 2 || parts.some((part) => !/^[A-Za-z0-9_-]+$/.test(part))) return invalid();
  const decode = (part: string) => JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(
    Uint8Array.from(atob(part.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0))));
  const header = decode(parts[0]);
  const claims = decode(parts[1]);
  if (!header || !claims || typeof header !== 'object' || typeof claims !== 'object'
    || header.alg !== (algorithm === 'es256' ? 'ES256' : 'EdDSA') || header.typ !== 'JWT' || header.kid !== key
    || typeof header.nonce !== 'string' || !header.nonce || header.nonce.length > 128 || /[\x00-\x1f\x7f]/.test(header.nonce)
    || claims.sub !== key || !Number.isSafeInteger(claims.nbf) || claims.nbf < 0
    || !Number.isSafeInteger(claims.exp) || claims.exp !== claims.nbf + 120) return invalid();
  const legacyEd = algorithm === 'ed25519' && claims.iss === 'cdp' && Array.isArray(claims.aud)
    && claims.aud.length === 1 && claims.aud[0] === 'cdp_service';
  if (!(claims.iss === 'cdp' && claims.aud === undefined) && !legacyEd
    && !(algorithm === 'es256' && claims.iss === 'coinbase-cloud' && claims.aud === undefined)) return invalid();
  if (claims.uri !== undefined && (typeof claims.uri !== 'string' || claims.uri.length > 4096
    || /[\x00-\x1f\x7f?#]/.test(claims.uri)
    || (!/^(GET|POST|PUT|PATCH|DELETE) api\.coinbase\.com\/api\/v3\/brokerage\//.test(claims.uri)
      && claims.uri !== 'POST drb.coinbase.com/api/v2/public/auth'))) return invalid();
  const canonical = `${encode({ alg: header.alg, typ: 'JWT', kid: key, nonce: header.nonce })}.${encode({ iss: claims.iss,
    ...(legacyEd && { aud: ['cdp_service'] }), nbf: claims.nbf, exp: claims.exp, sub: key,
    ...(claims.uri !== undefined && { uri: claims.uri }) })}`;
  if (canonical !== input) return invalid();
  return claims.uri === undefined ? {} : { uri: claims.uri };
};
