import { hashes, sign } from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2.js';
import bs58 from 'bs58';

const sha512ForEd25519: NonNullable<typeof hashes.sha512> = (message) => Uint8Array.from(sha512(message));

hashes.sha512 = sha512ForEd25519;

export const base64url = function (aStr: string): string {
  return aStr.replace(/\+/g, '-').replace(/\//g, '_');
};

export const orderlySign = (textSign: string, secret: string) => {
  const encoder = new TextEncoder();
  const signature = sign(encoder.encode(textSign), bs58.decode(secret.split(':')[1]));
  return base64url(Buffer.from(signature).toString('base64'));
};
