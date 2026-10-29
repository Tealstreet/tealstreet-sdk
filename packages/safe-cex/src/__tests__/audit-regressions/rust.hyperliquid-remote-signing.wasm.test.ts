import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeOptions, RemoteSigner, RemoteSignerRequest } from '../../types';

import { sha256 } from '@noble/hashes/sha2.js';
import { ethers } from 'ethers';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { assertBetaAccount } from '../../exchanges/rust/betaVenues';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider, remoteSignerAlgorithms } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { actionHash, signL1Action } from '../reference/hyperliquid/hyperliquid.signing';
import { prepareRecordedHttp } from '../support/managedHttp';

const ACCOUNT = '0x1111111111111111111111111111111111111111';
const VAULT = '0x3333333333333333333333333333333333333333';
const AGENT_KEY = '0x0123456789012345678901234567890123456789012345678901234567890123';
const AGENT = new ethers.Wallet(AGENT_KEY);
const IDENTITY = { credential: 'delegated/primary/credential', signing: 'delegated/primary/signing' };
const live: { dispose(): void }[] = [];
afterEach(() => {
  live.splice(0).forEach((item) => item.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});

/** The keyed device: an actual WASM engine answering through its hosted capture. */
const device = (venue: 'hyperliquid' | 'hyperliquidspot', options: Partial<ExchangeOptions> = {}) => {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('Signing capture must not use network'); });
  setRustEngineTransport(() => ({ prepareHttp: prepareRecordedHttp, now: () => 1_700_000_000_000, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange(venue, { engine: 'rust', key: ACCOUNT, secret: AGENT_KEY, ...options }, new DefaultStore());
  live.push(exchange);
  return { exchange, io };
};

/** The server: the keyless provider, wired to the device over the ws-auth wire. */
const server = (venue: 'hyperliquid' | 'hyperliquidspot', keyed: ReturnType<typeof device>) => {
  const sign = vi.fn((request: RemoteSignerRequest) => keyed.exchange.signWsAuth(request));
  const remoteSigner: RemoteSigner = { accountName: 'hosted-account', sign, isAvailable: () => true, onAvailabilityChange: () => () => {} };
  const signer = createRemoteSignerProvider({ exchangeName: venue, remoteSigner,
    identities: { primary: { ...IDENTITY, publicCredential: ACCOUNT } } });
  return { signer, sign };
};

const bytes = (text: string) => Array.from(new TextEncoder().encode(text));
const call = (signer: ReturnType<typeof server>['signer'], request: SignRequest) =>
  Promise.resolve(signer(request, { signal: new AbortController().signal }));
const context = { venue: 'hyperliquid', method: 'POST', host: 'api.hyperliquid.xyz', path: '/exchange' };
const bindAddress = { receipt: {}, context, operations: [{ kind: 'publicAddress', key: IDENTITY.signing }] } as unknown as SignRequest;
/** The engine's shape: statement commitment, address proof, digest signature. */
const work = (statement: string, digest: string): SignRequest => ({
  receipt: {},
  context,
  operations: [
    { kind: 'sha256', payload: [{ kind: 'bytes', bytes: bytes(statement) }] },
    { kind: 'publicAddress', key: IDENTITY.signing },
    { kind: 'sign', algorithm: 'secp256k1Digest', key: IDENTITY.signing, payload: [{ kind: 'bytes', bytes: Array.from(ethers.getBytes(digest)) }] },
  ],
} as unknown as SignRequest);
// The engine's canonical statement key order.
const statementOf = (action: unknown, nonce: number, vault: string | null, mainnet: boolean) =>
  JSON.stringify({ kind: 'l1', action, nonce, ...(vault === null ? {} : { vaultAddress: vault }), isMainnet: mainnet });
const digestOf = (action: unknown, nonce: number, vault: string | null, mainnet: boolean) => ethers.TypedDataEncoder.hash(
  { name: 'Exchange', version: '1', chainId: 1337, verifyingContract: '0x0000000000000000000000000000000000000000' },
  { Agent: [{ name: 'source', type: 'string' }, { name: 'connectionId', type: 'bytes32' }] },
  { source: mainnet ? 'a' : 'b', connectionId: actionHash(action, vault, nonce) },
);

const ACTIONS: Record<string, unknown> = {
  order: { type: 'order', orders: [
    { a: 4, b: true, p: '1234.5', s: '0.01', r: false, t: { limit: { tif: 'Alo' } }, c: '0x0000000000000000000000000000abcd' },
    { a: 4, b: false, p: '1100', s: '0.01', r: true, t: { trigger: { isMarket: true, triggerPx: '1100', tpsl: 'sl' } }, c: '0xffffffffffffffffffffffffffffffff' },
  ], grouping: 'normalTpsl', builder: { b: '0x1719884eb866cb12b2287399b15f7db5e7d775ea', f: 25 } },
  cancel: { type: 'cancel', cancels: [{ a: 4, o: 77 }, { a: 10_000, o: 2 ** 40 }] },
  cancelByCloid: { type: 'cancelByCloid', cancels: [{ asset: 4, cloid: '0x0000000000000000000000000000abcd' }] },
  modify: { type: 'modify', oid: 77, order: { a: 4, b: true, p: '1200', s: '0.02', r: false, t: { limit: { tif: 'Gtc' } } } },
  updateLeverage: { type: 'updateLeverage', asset: 4, isCross: false, leverage: 3 },
  scheduleCancel: { type: 'scheduleCancel', time: 1_700_000_100_000 },
};

describe('Hyperliquid remote signing over the structured ws-auth wire', () => {
  it('admits a keyless remoteSigner account and still refuses PayloadSigner', () => {
    const remoteSigner = { sign: vi.fn(), isAvailable: () => true, onAvailabilityChange: () => () => {} } as unknown as RemoteSigner;
    for (const venue of ['hyperliquid', 'hyperliquidspot'] as const) {
      expect(() => assertBetaAccount(venue, { key: ACCOUNT, remoteSigner })).not.toThrow();
      expect(remoteSignerAlgorithms(venue)).toEqual(['secp256k1Digest']);
      const exchange = new RustEngineExchange(venue, { engine: 'rust', key: ACCOUNT, secret: '', remoteSigner }, new DefaultStore());
      live.push(exchange);
      expect(exchange.isPublicDataOnly).toBe(false);
      expect(() => new RustEngineExchange(venue, { engine: 'rust', key: ACCOUNT, secret: '', payloadSigner: vi.fn() }, new DefaultStore()))
        .toThrow(/PayloadSigner/);
    }
  });

  it.each([
    ['hyperliquid', null, true], ['hyperliquid', null, false], ['hyperliquid', VAULT, true], ['hyperliquid', VAULT, false],
    ['hyperliquidspot', null, true], ['hyperliquidspot', VAULT, false],
  ] as const)('%s vault %s mainnet %s: device signatures equal local signing for every action', async (venue, vault, mainnet) => {
    const keyed = device(venue, { testnet: !mainnet, ...(vault === null ? {} : { vault }) });
    const { signer, sign } = server(venue, keyed);
    const address = Array.from(ethers.getBytes(AGENT.address));
    await expect(call(signer, bindAddress)).resolves.toEqual({ values: [address], receipt: {} });
    expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: venue, accountName: 'hosted-account',
      wsAuth: { toSign: ACCOUNT, outputFormat: 'hyperliquid-address' } }));
    const nonce = 1_700_000_000_123;
    for (const [name, action] of Object.entries(ACTIONS)) {
      const statement = statementOf(action, nonce, vault, mainnet);
      const result = await call(signer, work(statement, digestOf(action, nonce, vault, mainnet)));
      const local = await signL1Action(AGENT, action, vault, nonce, mainnet);
      const expected = [...ethers.getBytes(local.r), ...ethers.getBytes(local.s), local.v];
      expect(result.values, name).toEqual([Array.from(sha256(new TextEncoder().encode(statement))), address, expected]);
      expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ wsAuth: { toSign: statement, outputFormat: 'hyperliquid-l1' } }));
    }
    expect(keyed.io).not.toHaveBeenCalled();
  });

  it('the device refuses fund movement, another network or vault, other accounts and bare digests', async () => {
    const keyed = device('hyperliquid');
    const { signer } = server('hyperliquid', keyed);
    await call(signer, bindAddress);
    const nonce = 7;
    const cancel = ACTIONS.cancel;
    const refusals: [string, string][] = [
      ['withdraw', statementOf({ type: 'withdraw3', hyperliquidChain: 'Mainnet', signatureChainId: '0xa4b1', amount: '1', time: 1, destination: ACCOUNT }, nonce, null, true)],
      ['usdSend', statementOf({ type: 'usdSend', hyperliquidChain: 'Mainnet', signatureChainId: '0xa4b1', amount: '1', time: 1, destination: ACCOUNT }, nonce, null, true)],
      ['approveAgent', statementOf({ type: 'approveAgent', agentAddress: ACCOUNT, nonce: 1 }, nonce, null, true)],
      ['testnet', statementOf(cancel, nonce, null, false)],
      ['vault', statementOf(cancel, nonce, VAULT, true)],
      ['digest', digestOf(cancel, nonce, null, true)],
      ['expiring', statementOf(cancel, nonce, null, true).replace('"isMainnet"', '"expiresAfter":1,"isMainnet"')],
    ];
    for (const [name, statement] of refusals) {
      await expect(call(signer, work(statement, digestOf(cancel, nonce, null, true))), name).rejects.toThrow('Remote signing request refused');
      await expect(keyed.exchange.signWsAuth({ id: name, type: 'ws-auth', exchangeName: 'hyperliquid',
        wsAuth: { toSign: statement, outputFormat: 'hyperliquid-l1' } })).resolves.toMatchObject({ error: expect.any(String) });
    }
    // The address binding answers only for the signed-in account.
    await expect(keyed.exchange.signWsAuth({ id: 'other', type: 'ws-auth', exchangeName: 'hyperliquid',
      wsAuth: { toSign: VAULT, outputFormat: 'hyperliquid-address' } })).resolves.toMatchObject({ error: expect.any(String) });
    // An HMAC format never reaches the wallet key.
    await expect(keyed.exchange.signWsAuth({ id: 'hmac', type: 'ws-auth', exchangeName: 'hyperliquid',
      wsAuth: { toSign: statementOf(cancel, nonce, null, true), outputFormat: 'hex' } })).resolves.toMatchObject({ error: expect.any(String) });
  });

  it('the server never signs before binding an address, nor adopts a second device key', async () => {
    const keyed = device('hyperliquid');
    const { signer } = server('hyperliquid', keyed);
    const action = ACTIONS.cancel;
    await expect(call(signer, work(statementOf(action, 1, null, true), digestOf(action, 1, null, true))))
      .rejects.toThrow('Remote signing request refused');
    await call(signer, bindAddress);
    const other = device('hyperliquid', { secret: `0x${'22'.repeat(32)}` });
    const rebound = createRemoteSignerProvider({ exchangeName: 'hyperliquid',
      remoteSigner: { accountName: 'hosted-account', isAvailable: () => true, onAvailabilityChange: () => () => {},
        sign: vi.fn().mockImplementationOnce((request: RemoteSignerRequest) => keyed.exchange.signWsAuth(request))
          .mockImplementation((request: RemoteSignerRequest) => other.exchange.signWsAuth(request)) },
      identities: { primary: { ...IDENTITY, publicCredential: ACCOUNT } } });
    await call(rebound, bindAddress);
    await expect(call(rebound, bindAddress)).rejects.toThrow('Remote signing request refused');
  });
});
