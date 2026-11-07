import type { ExchangeOptions, RemoteSignerRequest } from '../../types';
import type { SignRequest } from '@tealstreet/safe-cex-wasm/host';

import { createHash } from 'node:crypto';
import { TypedDataEncoder, Wallet, getBytes } from 'ethers';
import { afterEach, expect, it, vi } from 'vitest';

import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { createRemoteSignerProvider } from '../../exchanges/rust/remoteSignerProvider';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';
import { prepareRecordedHttp } from '../support/managedHttp';

const now = 1_700_000_000_000;
const secret = `0x${'01'.repeat(32)}`;
const wallet = new Wallet(secret);
const subaccount = '9007199254740993';
const identity = { credential: 'origin/credential', signing: 'origin/signing', publicCredential: 'fixture-key' };
const live: RustEngineExchange[] = [];
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setWasmEngineLoader(loadNodeEngine);
  setRustEngineTransport();
});
function device(testnet: boolean) {
  setWasmEngineLoader(loadNodeEngine);
  const io = vi.fn(() => { throw new Error('No network in hosted parity fixture'); });
  setRustEngineTransport(() => ({ now: () => now, prepareHttp: prepareRecordedHttp, request: io, connect: io,
    setTimeout: (fn, ms) => setTimeout(fn, ms), clearTimeout: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>) }));
  const exchange = new RustEngineExchange('grvt', { key: 'fixture-key', secret, testnet,
    grvtTradingAccountId: subaccount } as ExchangeOptions, new DefaultStore());
  live.push(exchange);
  const logs: string[] = [];
  exchange.emitter.on('log', (message: string) => logs.push(message));
  return { exchange, io, logs };
}
function vector(testnet: boolean, nonce: number, expiry = now + 3_600_000, reduceOnly = false) {
  const statement = { subaccount, testnet, assetId: '030501'.padStart(64, '0'), amount: '1001000000',
    price: '50000100000000', buying: true, market: false, tif: 1, postOnly: false, reduceOnly,
    nonce, expiration: String(BigInt(expiry) * 1_000_000n) };
  const types = {
    OrderWithBuilderFee: [
      { name: 'subAccountID', type: 'uint64' }, { name: 'isMarket', type: 'bool' },
      { name: 'timeInForce', type: 'uint8' }, { name: 'postOnly', type: 'bool' }, { name: 'reduceOnly', type: 'bool' },
      { name: 'legs', type: 'OrderLeg[]' }, { name: 'builder', type: 'address' }, { name: 'builderFee', type: 'uint32' },
      { name: 'nonce', type: 'uint32' }, { name: 'expiration', type: 'int64' },
    ],
    OrderLeg: [{ name: 'assetID', type: 'uint256' }, { name: 'contractSize', type: 'uint64' },
      { name: 'limitPrice', type: 'uint64' }, { name: 'isBuyingContract', type: 'bool' }],
  };
  const digest = TypedDataEncoder.hash({ name: 'GRVT Exchange', version: '0', chainId: testnet ? 326 : 325 }, types, {
    subAccountID: subaccount, isMarket: false, timeInForce: 1, postOnly: false, reduceOnly,
    legs: [{ assetID: '0x030501', contractSize: statement.amount, limitPrice: statement.price, isBuyingContract: true }],
    builder: testnet ? '0xb0108ae4c07218ce320e5e22dd911a01c2042162' : '0xee8f3559f298fffa79b8253437969d15b5dccff3',
    builderFee: 300, nonce, expiration: statement.expiration,
  });
  const text = JSON.stringify(statement);
  const work: SignRequest = { receipt: { expiryMs: expiry }, context: { venue: 'advisory', method: '', host: '', path: '' },
    operations: [
      { kind: 'sha256', payload: [{ kind: 'bytes', bytes: Array.from(new TextEncoder().encode(text)) }] },
      { kind: 'publicAddress', key: identity.signing },
      { kind: 'sign', algorithm: 'secp256k1Digest', key: identity.signing, payload: [{ kind: 'bytes', bytes: Array.from(getBytes(digest)) }] },
    ] };
  return { text, digest, work };
}

it.each([false, true])('device WASM and local EIP-712 signatures match byte for byte (testnet=%s)', async (testnet) => {
  const browser = device(testnet);
  const sign = vi.fn((request: RemoteSignerRequest) => browser.exchange.signWsAuth(request));
  const provider = createRemoteSignerProvider({ exchangeName: 'grvt', identities: { primary: identity },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  for (const nonce of [0, 42, 0xffffffff]) for (const reduceOnly of [false, true]) {
    const { text, digest, work } = vector(testnet, nonce, now + 3_600_000, reduceOnly);
    const result = await provider(work, { signal: new AbortController().signal });
    expect(result.values).toEqual([
      Array.from(createHash('sha256').update(text).digest()), Array.from(getBytes(wallet.address)),
      [...getBytes(wallet.signingKey.sign(digest).serialized).slice(0, 64), wallet.signingKey.sign(digest).yParity],
    ]);
    expect(sign).toHaveBeenLastCalledWith(expect.objectContaining({ type: 'ws-auth', exchangeName: 'grvt',
      wsAuth: { toSign: text, outputFormat: 'grvt-order' } }));
  }
  expect(browser.io).not.toHaveBeenCalled();
  expect(JSON.stringify(sign.mock.calls)).not.toContain(secret);
  expect(JSON.stringify(sign.mock.calls)).not.toContain('origin/');
  expect(JSON.stringify(browser.logs)).not.toContain(secret);
  for (const field of ['key', 'secret', 'remoteSigner']) expect(browser.exchange.options).not.toHaveProperty(field);
});

it('device refuses unknown/cross-account/network/expired descriptors and raw digest requests without leaking input', async () => {
  const browser = device(false);
  const { text } = vector(false, 42);
  for (const toSign of [text.replace(subaccount, '9007199254740992'), text.replace('"testnet":false', '"testnet":true'),
    text.replace('{', '{"unknown":true,'), vector(false, 42, now).text, secret]) {
    const result = await browser.exchange.signWsAuth({ id: 'refused', type: 'ws-auth', exchangeName: 'grvt',
      wsAuth: { toSign, outputFormat: 'grvt-order' } });
    expect(result.wsAuthSignature).toBeUndefined();
    expect(result.error).toBe('Hosted WS auth signing failed');
  }
  expect(browser.io).not.toHaveBeenCalled();
  expect(JSON.stringify(browser.logs)).not.toContain(secret);
});

it('actual keyless native origin and local custody emit identical login/session/order bytes and preserve Unknown', async () => {
  const { EngineHost } = await import('@tealstreet/safe-cex-wasm/host');
  const { WasmEngine } = await loadNodeEngine();
  const browser = device(false);
  let refuse = false;
  const sign = vi.fn((request: RemoteSignerRequest) => {
    if (refuse) throw new Error(secret);
    return browser.exchange.signWsAuth(request);
  });
  const provider = createRemoteSignerProvider({ exchangeName: 'grvt', identities: { primary: identity },
    remoteSigner: { sign, isAvailable: () => true, onAvailabilityChange: () => () => {} } });
  const runs: Array<Array<{ method: string; url: string; headers: Record<string, string>; body?: string }>> = [];
  const socketRuns: Array<Array<{ url: string; headers?: Record<string, string>; sent: string[] }>> = [];
  for (const mode of ['local', 'remote', 'refused']) {
    const remote = mode !== 'local';
    refuse = mode === 'refused';
    const engine = new WasmEngine('grvt', JSON.stringify({
      ...(remote ? { signing: { primary: { credential: identity.credential, signing: identity.signing } }, remoteSigning: true,
        grvtSignerAddress: wallet.address } : { key: 'fixture-key', secret }),
      grvtTradingAccountId: subaccount, grvtSessionProxy: true,
    }));
    const packets: typeof runs[number] = [];
    const sockets: typeof socketRuns[number] = [];
    const host = new EngineHost(engine, { now: () => now, prepareHttp: prepareRecordedHttp,
      request: async ({ method, url, headers, body }) => {
        packets.push({ method, url, headers, body });
        if (url.endsWith('create_order')) throw new Error('fixture transport loss');
        const data = url.endsWith('all_instruments')
          ? '{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}'
          : url.endsWith('login') ? `{"sub_account_id":"${subaccount}"}`
          : url.endsWith('aggregated_account_summary') ? `{"r":{"te":"1000","ts2":"1000","tu":"0","sas":[{"si":"${subaccount}"}]}}`
          : url.endsWith('get_authorized_builders') ? '{"results":[{"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}]}'
          : '{"r":[]}';
        return { status: 200, headers: url.endsWith('login') ? 'Set-Cookie: gravity=fixture-session; Path=/\nX-Grvt-Account-Id: fixture-account' : '',
          body: new TextEncoder().encode(data) };
      },
      connect: (url, handlers) => {
        const socket = { url, headers: handlers.headers, sent: [] as string[] };
        if (handlers.channel === 'private') sockets.push(socket);
        queueMicrotask(() => handlers.onOpen());
        return { send: (text) => {
          if (handlers.channel === 'private') socket.sent.push(text);
          if (text.includes('"type":"auth"')) queueMicrotask(() => handlers.onMessage('{"type":"auth_success"}'));
        }, close: () => {} };
      },
      setTimeout: () => 0, clearTimeout: () => {},
    }, {}, { ownsEngine: true, entropy: (count) => new Uint8Array(count).fill(7),
      signingEntropy: (count) => new Uint8Array(count).fill(7),
      ...(remote ? { signer: provider, signerAlgorithms: ['secp256k1Digest'] } : {}) });
    try {
      host.start();
      await vi.waitFor(() => expect(packets.some(({ url }) => url.endsWith('get_authorized_builders'))).toBe(true));
      await vi.waitFor(() => expect(sockets.some(({ sent }) => sent.some((text) => text.includes('"type":"auth"')))).toBe(true));
      const failure = await host.command({ op: 'placeOrders', orders: [{ symbol: 'BTC_USDT_PERP', type: 'limit', side: 'buy', amount: 1,
        price: 100, clientOrderId: '99' }] }).catch((error: unknown) => error);
      expect(failure).toMatchObject({ outcome: refuse ? 'notSent' : 'unknown' });
      expect(String(failure)).not.toContain(secret);
      expect(packets.filter(({ url }) => url.endsWith('create_order'))).toHaveLength(refuse ? 0 : 1);
      if (!refuse) { runs.push(packets); socketRuns.push(sockets); }
    } finally { host.dispose(); }
  }
  expect(runs[1]).toEqual(runs[0]);
  expect(socketRuns[1]).toEqual(socketRuns[0]);
  expect(runs[0].find(({ url }) => url.endsWith('login'))?.body).toBe('{"api_key":"fixture-key"}');
  expect(sign).toHaveBeenCalledTimes(2);
  expect(JSON.stringify(sign.mock.calls)).not.toContain(secret);
});
