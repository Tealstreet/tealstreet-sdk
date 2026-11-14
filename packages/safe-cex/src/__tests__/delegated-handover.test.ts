// @vitest-environment node
import type { DelegatedHandoverInput } from '../delegatedHandover';

import { Buffer } from 'node:buffer';
import { createDecipheriv, createHmac, createPrivateKey, createPublicKey, diffieHellman, hkdfSync } from 'node:crypto';

import { x25519 } from '@noble/curves/ed25519.js';
import { hexToBytes } from '@noble/hashes/utils.js';
import { afterEach, describe, expect, it, vi } from 'vitest';

import fixed from '../../../../sdk-rs/hosts/server/tests/fixtures/delegated-handover-vectors.json';
import wallet from '../../../../sdk-rs/hosts/server/tests/fixtures/delegated-wallet-handover.json';
import { sealDelegatedHandover } from '../delegatedHandover';
import { sealHandoverHpke } from '../delegatedHandoverHpke';
import rfc from './fixtures/rfc9180-x25519-chacha20poly1305.json';

vi.mock('@noble/curves/ed25519.js', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@noble/curves/ed25519.js')>();
  return {
    ...actual,
    x25519: {
      ...actual.x25519,
      utils: { ...actual.x25519.utils, randomSecretKey: vi.fn(actual.x25519.utils.randomSecretKey) },
    },
  };
});

const recipientSeed = new Uint8Array(32).fill(17);
const input: DelegatedHandoverInput = {
  publicKey: Buffer.from(x25519.getPublicKey(recipientSeed)).toString('base64'),
  attemptId: 'synthetic-attempt-A',
  bootId: 'synthetic-boot-A',
  exchangeName: 'bybit',
  key: 'synthetic-key-A',
  secret: 'synthetic-secret-A',
};
const info = Buffer.from('tealstreet/delegated-handover/v1');
const aad = (value: DelegatedHandoverInput) =>
  Buffer.from([value.attemptId, value.bootId, value.exchangeName, value.key].join('|'));

// Independent Node-crypto receiver for the single-message interoperability boundary.
function open(sealed: ReturnType<typeof sealDelegatedHandover>, context = input): unknown {
  const enc = Buffer.from(sealed.enc, 'base64');
  const receiver = createPrivateKey({
    key: Buffer.concat([Buffer.from('302e020100300506032b656e04220420', 'hex'), recipientSeed]),
    type: 'pkcs8',
    format: 'der',
  });
  const sender = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b656e032100', 'hex'), enc]),
    type: 'spki',
    format: 'der',
  });
  const dh = diffieHellman({ privateKey: receiver, publicKey: sender });
  const kem = Buffer.from([75, 69, 77, 0, 32]);
  const suite = Buffer.from([72, 80, 75, 69, 0, 32, 0, 1, 0, 3]);
  const label = (suiteId: Buffer, name: string, data: Uint8Array) =>
    Buffer.concat([Buffer.from('HPKE-v1'), suiteId, Buffer.from(name), data]);
  const expandInfo = (suiteId: Buffer, name: string, data: Uint8Array, length: number) =>
    Buffer.concat([Buffer.from([0, length]), label(suiteId, name, data)]);
  const shared = Buffer.from(
    hkdfSync(
      'sha256',
      label(kem, 'eae_prk', dh),
      Buffer.alloc(0),
      expandInfo(kem, 'shared_secret', Buffer.concat([enc, Buffer.from(context.publicKey, 'base64')]), 32),
      32,
    ),
  );
  const hash = (name: string, data: Uint8Array) =>
    createHmac('sha256', Buffer.alloc(0))
      .update(label(suite, name, data))
      .digest();
  const schedule = Buffer.concat([Buffer.from([0]), hash('psk_id_hash', Buffer.alloc(0)), hash('info_hash', info)]);
  const secret = createHmac('sha256', shared)
    .update(label(suite, 'secret', Buffer.alloc(0)))
    .digest();
  const expand = (name: string, length: number) =>
    createHmac('sha256', secret)
      .update(expandInfo(suite, name, schedule, length))
      .update(Buffer.from([1]))
      .digest()
      .subarray(0, length);
  const ciphertext = Buffer.from(sealed.ciphertext, 'base64');
  const decipher = createDecipheriv('chacha20-poly1305', expand('key', 32), expand('base_nonce', 12), {
    authTagLength: 16,
  });
  decipher.setAAD(aad(context), { plaintextLength: ciphertext.length - 16 });
  decipher.setAuthTag(ciphertext.subarray(-16));
  return JSON.parse(Buffer.concat([decipher.update(ciphertext.subarray(0, -16)), decipher.final()]).toString('utf8'));
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe('DELEGATED handover seal', () => {
  it('keeps key-only signing material exclusively inside the encrypted full init', () => {
    vi.mocked(x25519.utils.randomSecretKey).mockReturnValueOnce(new Uint8Array(32).fill(wallet.senderSeedByte));
    const sealed = sealDelegatedHandover(wallet.input);
    expect(sealed).toEqual(wallet.frame.d.sealed);
    const frame = { ...wallet.frame, d: { ...wallet.frame.d, sealed } };
    expect(JSON.stringify(frame)).not.toContain(wallet.input.signingKey);
    expect(aad(wallet.input).toString()).not.toContain(wallet.input.signingKey);
    expect(open(sealed, wallet.input)).toEqual({
      key: wallet.input.key, secret: '', signingKey: wallet.input.signingKey,
    });
  });
  it('seals a key-only wallet shape for trusted server-side credential validation', () => {
    const context = wallet.input;
    expect(open(sealDelegatedHandover(context), context)).toEqual({
      key: context.key, secret: '', signingKey: context.signingKey,
    });
  });
  it('refuses an empty private signing field', () => {
    expect(() => sealDelegatedHandover({ ...wallet.input, signingKey: '' })).toThrow('Unable to seal');
  });
  it('seals the requested non-Bybit venue with the unchanged venue-bound proof', () => {
    const context = { ...input, exchangeName: 'binance' };
    const sealed = sealDelegatedHandover(context);
    expect(open(sealed, context)).toEqual({ key: input.key, secret: input.secret });
    expect(() => open(sealed, input)).toThrow();
  });
  it('matches RFC 9180 appendix A.2.1 sequence zero', () => {
    vi.mocked(x25519.utils.randomSecretKey).mockReturnValueOnce(hexToBytes(rfc.skEm));
    const sealed = sealHandoverHpke(
      hexToBytes(rfc.pkRm),
      hexToBytes(rfc.info),
      hexToBytes(rfc.encryption.aad),
      hexToBytes(rfc.encryption.pt),
    );
    expect(Buffer.from(sealed.enc).toString('hex')).toBe(rfc.enc);
    expect(Buffer.from(sealed.ciphertext).toString('hex')).toBe(rfc.encryption.ct);
  });

  for (const vector of fixed.vectors) {
    it(`matches shared ${vector.name} wire bytes and opens with an independent receiver`, () => {
      vi.mocked(x25519.utils.randomSecretKey).mockReturnValueOnce(new Uint8Array(32).fill(vector.senderSeedByte));
      const sealed = sealDelegatedHandover(vector.input);
      expect(sealed).toEqual(vector.sealed);
      expect(open(sealed, vector.input)).toEqual(
        JSON.parse(
          JSON.stringify({
            key: vector.input.key,
            secret: vector.input.secret,
            passphrase: vector.input.passphrase,
          }),
        ),
      );
    });
  }

  it.each([undefined, '', 'synthetic-passphrase-雪'])(
    'seals exact credential bytes with passphrase %s',
    (passphrase) => {
      const credentials = { ...input, key: 'synthetic-key-雪', secret: ' synthetic-secret-雪 ', passphrase };
      const sealed = sealDelegatedHandover(credentials);
      expect(Object.keys(sealed).sort()).toEqual(['attemptId', 'ciphertext', 'enc']);
      expect(sealed.attemptId).toBe(credentials.attemptId);
      expect(open(sealed, credentials)).toEqual(
        JSON.parse(JSON.stringify({ key: credentials.key, secret: credentials.secret, passphrase })),
      );
    },
  );

  it('uses fresh sender keys for each seal', () => {
    const first = sealDelegatedHandover(input);
    const second = sealDelegatedHandover(input);
    expect(first.enc).not.toBe(second.enc);
    expect(first.ciphertext).not.toBe(second.ciphertext);
    expect(open(first)).toEqual({ key: input.key, secret: input.secret });
    expect(open(second)).toEqual({ key: input.key, secret: input.secret });
  });

  it.each(['attemptId', 'bootId', 'exchangeName', 'key'] as const)('authenticates %s', (field) => {
    const sealed = sealDelegatedHandover(input);
    expect(() => open(sealed, { ...input, [field]: `${input[field]}-different` })).toThrow();
  });

  it('keeps sequential owners independent without shared state', () => {
    const first = sealDelegatedHandover(input);
    const secondInput = {
      ...input,
      attemptId: 'synthetic-attempt-B',
      key: 'synthetic-key-B',
      secret: 'synthetic-secret-B',
    };
    const second = sealDelegatedHandover(secondInput);
    expect(open(second, secondInput)).toEqual({ key: secondInput.key, secret: secondInput.secret });
    expect(open(first)).toEqual({ key: input.key, secret: input.secret });
    expect(() => open(first, secondInput)).toThrow();
  });

  it('works without Buffer or browser base64 globals', () => {
    vi.stubGlobal('Buffer', undefined);
    vi.stubGlobal('atob', undefined);
    vi.stubGlobal('btoa', undefined);
    expect(open(sealDelegatedHandover(input))).toEqual({ key: input.key, secret: input.secret });
  });

  // Eligibility belongs to the host; malformed proof contexts always refuse.
  it.each(['', 'binance|other', String.fromCharCode(0xd800)])('refuses invalid venue context %s', (exchangeName) => {
    expect(() => sealDelegatedHandover({ ...input, exchangeName })).toThrow('DELEGATED');
  });

  it.each(['attemptId', 'bootId', 'key'] as const)('requires an unambiguous %s', (field) => {
    expect(() => sealDelegatedHandover({ ...input, [field]: 'synthetic|value' })).toThrow();
    expect(() => sealDelegatedHandover({ ...input, [field]: '' })).toThrow();
    expect(() => sealDelegatedHandover({ ...input, [field]: '\ud800' })).toThrow();
  });

  it.each(['', 'not base64', 'AA==', input.publicKey.replace(/=$/, ''), `${input.publicKey}\n`, 'A'.repeat(43) + '='])(
    'validates recipient encoding',
    (publicKey) => {
      expect(() => sealDelegatedHandover({ ...input, publicKey })).toThrow();
    },
  );

  it('does not include caller material in a failure message', () => {
    vi.mocked(x25519.utils.randomSecretKey).mockImplementationOnce(() => {
      throw new Error(input.secret);
    });
    expect(() => sealDelegatedHandover(input)).toThrow('Unable to seal DELEGATED handover');
  });

  it('clears the ephemeral sender bytes after sealing', () => {
    const ephemeral = new Uint8Array(32).fill(34);
    vi.mocked(x25519.utils.randomSecretKey).mockReturnValueOnce(ephemeral);
    sealDelegatedHandover(input);
    expect(ephemeral.every((byte) => byte === 0)).toBe(true);
  });

  it('clears the ephemeral sender bytes when the recipient is refused', () => {
    const ephemeral = new Uint8Array(32).fill(34);
    vi.mocked(x25519.utils.randomSecretKey).mockReturnValueOnce(ephemeral);
    expect(() => sealDelegatedHandover({ ...input, publicKey: Buffer.alloc(32).toString('base64') })).toThrow();
    expect(ephemeral.every((byte) => byte === 0)).toBe(true);
  });
});
