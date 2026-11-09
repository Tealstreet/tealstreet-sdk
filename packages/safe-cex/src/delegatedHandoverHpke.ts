import { chacha20poly1305 } from '@noble/ciphers/chacha.js';
import { x25519 } from '@noble/curves/ed25519.js';
import { expand, extract } from '@noble/hashes/hkdf.js';
import { sha256 } from '@noble/hashes/sha2.js';
import { concatBytes, utf8ToBytes } from '@noble/hashes/utils.js';

const EMPTY = new Uint8Array();
const VERSION = utf8ToBytes('HPKE-v1');
const KEM_SUITE = concatBytes(utf8ToBytes('KEM'), new Uint8Array([0, 32]));
const HPKE_SUITE = concatBytes(utf8ToBytes('HPKE'), new Uint8Array([0, 32, 0, 1, 0, 3]));

const labeledExtract = (suite: Uint8Array, salt: Uint8Array, label: string, ikm: Uint8Array) => {
  const labeledIkm = concatBytes(VERSION, suite, utf8ToBytes(label), ikm);
  try {
    return extract(sha256, labeledIkm, salt);
  } finally {
    labeledIkm.fill(0);
  }
};

const labeledExpand = (suite: Uint8Array, prk: Uint8Array, label: string, info: Uint8Array, length: number) =>
  expand(
    sha256,
    prk,
    concatBytes(new Uint8Array([length >> 8, length & 255]), VERSION, suite, utf8ToBytes(label), info),
    length,
  );

/** Internal, sequence-zero RFC 9180 base sender: X25519/HKDF-SHA256/ChaCha20-Poly1305. */
export function sealHandoverHpke(
  publicKey: Uint8Array,
  info: Uint8Array,
  aad: Uint8Array,
  plaintext: Uint8Array,
): { enc: Uint8Array; ciphertext: Uint8Array } {
  const sensitive: Uint8Array[] = [];
  const track = (bytes: Uint8Array) => {
    sensitive.push(bytes);
    return bytes;
  };
  try {
    const ephemeral = track(x25519.utils.randomSecretKey());
    const enc = x25519.getPublicKey(ephemeral);
    const dh = track(x25519.getSharedSecret(ephemeral, publicKey));
    const eaePrk = track(labeledExtract(KEM_SUITE, EMPTY, 'eae_prk', dh));
    const sharedSecret = track(labeledExpand(KEM_SUITE, eaePrk, 'shared_secret', concatBytes(enc, publicKey), 32));
    const context = concatBytes(
      new Uint8Array([0]),
      labeledExtract(HPKE_SUITE, EMPTY, 'psk_id_hash', EMPTY),
      labeledExtract(HPKE_SUITE, EMPTY, 'info_hash', info),
    );
    const secret = track(labeledExtract(HPKE_SUITE, sharedSecret, 'secret', EMPTY));
    const key = track(labeledExpand(HPKE_SUITE, secret, 'key', context, 32));
    const nonce = track(labeledExpand(HPKE_SUITE, secret, 'base_nonce', context, 12));
    return { enc, ciphertext: chacha20poly1305(key, nonce, aad).encrypt(plaintext) };
  } finally {
    for (const bytes of sensitive) bytes.fill(0);
  }
}
