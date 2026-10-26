import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';

export function sha256Hex(inputString: string): string {
  return bytesToHex(sha256(utf8ToBytes(inputString)));
}
