import cryptojs from 'crypto-js';

export function sha256Hex(inputString: string): string {
  return cryptojs.SHA256(inputString).toString(cryptojs.enc.Hex);
}
