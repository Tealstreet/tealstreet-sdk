import { hmac } from '@noble/hashes/hmac.js';
import { sha256 } from '@noble/hashes/sha2.js';
import { utf8ToBytes } from '@noble/hashes/utils.js';

// HMAC-SHA256 of `message` keyed by `key` (both interpreted as UTF-8), output
// as standard (padded) base64. Byte-for-byte compatible with the previous
// crypto-js `HmacSHA256(...).toString(enc.Base64)` path — locked by
// src/__tests__/crypto-js-migration.characterization.test.ts against captured
// crypto-js values.
export function hmacSha256Base64(message: string, key: string): string {
  return Buffer.from(hmac(sha256, utf8ToBytes(key), utf8ToBytes(message))).toString('base64');
}

export function getPrecision(num: number | string) {
  const val = Number(num);
  if (isNaN(val)) {
    return 0;
  }

  const strList = String(num).split('.');
  return strList.length === 2 ? strList[1].length : 0;
}

export function generateApeXRandomClientId() {
  return 'apexomni-tealstreet-' + Math.random().toString().slice(2).replace(/^0+/, '');
}

export function dropPrefix(hash: string) {
  return hash.replace(/^0x/, '');
}
