import type { HostedSigningInput, HostedSigningOutput } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, RemoteSignerRequest } from '../../types';

import bs58 from 'bs58';
import { expect, it, vi } from 'vitest';

import { BaseExchange } from '../../exchanges/base';
import { DefaultStore } from '../../store/store.base';
import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { deriveRemoteSigningPolicy } from '../../exchanges/rust/construction';
import { signHostedWsAuth } from '../../exchanges/rust/legacySigning';
import golden from '../fixtures/orderly-device-signing.json';
import { orderlySign } from '../reference/orderly/orderly.encryption';

const aliases: readonly ExchangeName[] = ['kodiakfi', 'mode', 'what', 'woofi'];

it('pins independent browser Ed25519 goldens for exact REST, WS and corrected-clock payloads', () => {
  const syntheticSeed = `ed25519:${bs58.encode(new Uint8Array(32).fill(7))}`;
  for (const row of golden.vectors) {
    expect(orderlySign(row.payload, syntheticSeed), row.name).toBe(row.signature);
    expect(row.signature).toMatch(/^[A-Za-z0-9_-]{86}==$/);
  }
});

it.each(aliases)('%s: dispatches the closed Orderly device format exactly once without synthesizing HTTP or time', async (exchangeName) => {
  const row = golden.vectors.find(vector => vector.name === 'read');
  if (!row) throw new Error('Missing frozen read fixture');
  const toSign = JSON.stringify({ version: 'orderly/1', venue: exchangeName, slot: 'primary',
    kind: row.kind, stampMs: row.stampMs, payload: row.payload });
  const request: RemoteSignerRequest = { id: 'fixture-id', type: 'ws-auth', exchangeName,
    wsAuth: { toSign, outputFormat: 'orderly-ed25519' } };
  const batch = JSON.stringify({ credential: golden.publicKey, signature: row.signature });
  const hostedSigning = vi.fn(async (_input: HostedSigningInput): Promise<HostedSigningOutput> => ({ signature: batch }));
  const hostedWalletSigning = vi.fn(() => { throw new Error('Orderly must not use the wallet codec'); });
  const output = await signHostedWsAuth({ hostedSigning, hostedWalletSigning }, request);
  expect(output.error).toBeUndefined();
  expect(output.id === request.id && output.wsAuthSignature === batch).toBe(true);
  expect(hostedSigning).toHaveBeenCalledExactlyOnceWith({ kind: 'wsAuth', toSign, outputFormat: 'orderly-ed25519' });
  expect(hostedWalletSigning).not.toHaveBeenCalled();
});

it.each(aliases)('%s: codec declarations confer no private Hosted admission', (name) => {
  const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} };
  expect(deriveRemoteSigningPolicy(name)?.enabled).not.toBe(true);
  expect(() => assertBetaAccount(name, { key: golden.publicKey, remoteSigner })).toThrow(
    expect.objectContaining({ kind: 'unsupported', outcome: 'notSent' }),
  );
  expect(() => assertBetaAccount(name, { key: golden.publicKey, remoteSigner, publicDataOnly: true })).not.toThrow();
  expect(remoteSigner.sign).not.toHaveBeenCalled();
});

class LegacySignerControl extends BaseExchange {}

it('the base signer refuses the Orderly tag instead of returning an HMAC', async () => {
  const exchange = new LegacySignerControl('mode', { key: 'synthetic-public', secret: 'synthetic-control' }, new DefaultStore());
  try {
    const output = await exchange.signWsAuth({ id: 'control', type: 'ws-auth', exchangeName: 'mode',
      wsAuth: { toSign: '1700000000123', outputFormat: 'orderly-ed25519' } });
    expect(output).toEqual({ id: 'control', error: 'orderly-ed25519 requires the Rust engine signer' });
  } finally { exchange.dispose(); }
});
