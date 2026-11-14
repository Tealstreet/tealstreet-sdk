import { utf8ToBytes } from '@noble/hashes/utils.js';

import { sealHandoverHpke } from './delegatedHandoverHpke';

export type DelegatedHandoverInput = Readonly<{
  publicKey: string;
  attemptId: string;
  bootId: string;
  exchangeName: string;
  /** Public init identity; key-only setup uses its public subkeyAddress. */
  key: string;
  secret: string;
  passphrase?: string;
  /** Private key-only material; never copy into unsealed account settings. */
  signingKey?: string;
}>;

export type SealedDelegatedHandover = Readonly<{
  attemptId: string;
  enc: string;
  ciphertext: string;
}>;

const INFO = utf8ToBytes('tealstreet/delegated-handover/v1');
const BASE64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';

// Keep the wire encoding independent of Node Buffer and browser-only globals.
function base64(bytes: Uint8Array): string {
  let encoded = '';
  for (let i = 0; i < bytes.length; i += 3) {
    const word = (bytes[i] << 16) | ((bytes[i + 1] ?? 0) << 8) | (bytes[i + 2] ?? 0);
    encoded += BASE64[(word >>> 18) & 63] + BASE64[(word >>> 12) & 63];
    encoded += i + 1 < bytes.length ? BASE64[(word >>> 6) & 63] : '=';
    encoded += i + 2 < bytes.length ? BASE64[word & 63] : '=';
  }
  return encoded;
}

function recipientBytes(publicKey: string): Uint8Array {
  if (!/^[A-Za-z0-9+/]{43}=$/.test(publicKey)) throw new Error('Invalid DELEGATED public key');
  const bytes = new Uint8Array(32);
  let cursor = 0;
  for (let i = 0; i < publicKey.length; i += 4) {
    const word =
      (BASE64.indexOf(publicKey[i]) << 18) |
      (BASE64.indexOf(publicKey[i + 1]) << 12) |
      ((publicKey[i + 2] === '=' ? 0 : BASE64.indexOf(publicKey[i + 2])) << 6) |
      (publicKey[i + 3] === '=' ? 0 : BASE64.indexOf(publicKey[i + 3]));
    bytes[cursor++] = word >>> 16;
    if (cursor < bytes.length) bytes[cursor++] = (word >>> 8) & 255;
    if (cursor < bytes.length) bytes[cursor++] = word & 255;
  }
  if (base64(bytes) !== publicKey) throw new Error('Invalid DELEGATED public key');
  return bytes;
}

function validContext(value: unknown): value is string {
  if (typeof value !== 'string' || value.length === 0 || value.includes('|')) return false;
  // UTF-8 must preserve the supplied context, including non-ASCII identities.
  for (const character of value) {
    const point = character.codePointAt(0)!;
    if (point >= 0xd800 && point <= 0xdfff) return false;
  }
  return true;
}

/** Seal one DELEGATED attempt using the host-provided ephemeral public key.
 * Input and output key/ciphertext encodings are canonical padded base64.
 * The caller owns venue enablement and the attempt's connection/lifecycle.
 */
export function sealDelegatedHandover(input: DelegatedHandoverInput): SealedDelegatedHandover {
  let plaintext: Uint8Array | undefined;
  try {
    const { publicKey, attemptId, bootId, exchangeName, key, secret, passphrase, signingKey } = input;
    if (
      !validContext(exchangeName) ||
      !validContext(attemptId) ||
      !validContext(bootId) ||
      !validContext(key) ||
      typeof publicKey !== 'string' ||
      typeof secret !== 'string' ||
      (passphrase !== undefined && typeof passphrase !== 'string') ||
      (signingKey !== undefined && (typeof signingKey !== 'string' || signingKey.length === 0))
    ) {
      throw new Error('Invalid DELEGATED handover');
    }
    const recipient = recipientBytes(publicKey);
    const aad = utf8ToBytes([attemptId, bootId, exchangeName, key].join('|'));
    plaintext = utf8ToBytes(JSON.stringify({ key, secret, passphrase, signingKey }));
    const sealed = sealHandoverHpke(recipient, INFO, aad, plaintext);
    return { attemptId, enc: base64(sealed.enc), ciphertext: base64(sealed.ciphertext) };
  } catch {
    throw new Error('Unable to seal DELEGATED handover');
  } finally {
    plaintext?.fill(0);
  }
}
