import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { createHmac } from 'node:crypto';
import { ethers } from 'ethers';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { BETA_VENUES } from '../../exchanges/rust/betaVenues';
import vectors from '../../../../../sdk-rs/venues/paradex/tests/sign-vectors.json';
import rules from '../../exchanges/rust/construction-rules.json';
import { loadNodeEngine } from '../../exchanges/rust/loadNodeEngine';
import { RustEngineExchange, setRustEngineTransport, setWasmEngineLoader } from '../../exchanges/rust/rust.exchange';
import { DefaultStore } from '../../store/store.base';

// The COSIGNED gate for page signing (NX `pageSigns`): every frame-fed family's
// page engine, built as the web controller builds it, answers the sign-reqs its
// native server sends. A family joining FRAME_FED_VENUES without a case here fails.
const STAMP = 1_700_000_000_123;
const KEY = 'page-primary-key';
const MAC_KEY = 'page-primary-mac-key';
const PASSPHRASE = 'page-passphrase';
const WALLET = '0x1111111111111111111111111111111111111111';
const AGENT_KEY = '0x0123456789012345678901234567890123456789012345678901234567890123';
const live: RustEngineExchange[] = [];

type Rules = { venues: Record<string, { remoteSigning?: { signatureEncoding?: string }; signerSetup?: { primary?: { passphrase?: string } } }> };
const venueRules = (name: string) => (rules as Rules).venues[name] ?? {};

interface SigningCase {
  options: Partial<ExchangeOptions>;
  /** The device clock, where the venue's statements carry times. */
  now?: number;
  /** Sign-reqs as the native server's remote signer sends them, with the expected answer. */
  requests: { outputFormat: string; toSign: string; expected: string | RegExp }[];
}

/** HMAC families: every REST and socket signature is one ws-auth `toSign` in the venue's declared encoding. */
const hmac = (name: string): SigningCase => {
  const encoding = venueRules(name).remoteSigning?.signatureEncoding ?? 'hex';
  const toSign = `${STAMP}GET/private/audit?symbol=BTCUSDT`;
  const bytes = createHmac('sha256', MAC_KEY).update(toSign).digest();
  const expected =
    encoding === 'base64'
      ? bytes.toString('base64')
      : encoding === 'hex-base64'
        ? Buffer.from(bytes.toString('hex')).toString('base64')
        : bytes.toString('hex');
  const passphrase = venueRules(name).signerSetup?.primary?.passphrase === 'requiredForPrivate';
  return {
    options: { key: KEY, secret: MAC_KEY, ...(passphrase ? { passphrase: PASSPHRASE } : {}) },
    requests: [{ outputFormat: encoding, toSign, expected }],
  };
};

/** Wallet families: startup binds the agent address before any L1 action is signed. */
const wallet = (): SigningCase => ({
  options: { key: WALLET, secret: AGENT_KEY },
  requests: [
    { outputFormat: 'hyperliquid-address', toSign: WALLET, expected: new ethers.Wallet(AGENT_KEY).address.toLowerCase() },
  ],
});

/** Paradex: the native server binds the Stark public key, then sends `paradex/1` statements. */
const paradex = (): SigningCase => {
  const vector = vectors.cases.find((item) => item.environment === 'prod')!;
  const nowS = Math.floor(vector.now / 1000);
  const statement = JSON.stringify({ family: 'paradex', op: 'auth', revision: 'paradex/1', timestampS: nowS, expirationS: nowS + 300 });
  // starknet.js `["r","s"]` felts as the device's `0x` r || s.
  const rs = (JSON.parse(vector.shortAuth.signature) as string[]).map((felt) => BigInt(felt).toString(16).padStart(64, '0')).join('');
  return {
    now: vector.now,
    options: { key: vector.key, secret: '', paradexAddress: vector.address, subkeyAddress: vector.subkey },
    requests: [
      // The Stark generator's x: the public key of the vector's scalar 1.
      { outputFormat: 'paradex-public', toSign: vector.address, expected: '0x01ef15c18599971b7beced415a40f0c7deacfd9b0d1819e03d723d8bc943cfca' },
      { outputFormat: 'paradex-stark', toSign: statement, expected: `0x${rs}` },
    ],
  };
};

/** Lighter: the native server sends canonical statements as `lighter-tx`; answers are 80-byte Schnorr signatures. */
const lighter = (): SigningCase => {
  // The device checks Lighter deadlines against its own wall clock.
  const deadline = Math.floor(Date.now() / 1000) + 60;
  const auth = (account: number) => `{"kind":"auth","chainId":300,"accountIndex":${account},"apiKeyIndex":0,"deadline":${deadline}}`;
  return {
    now: Date.now(),
    options: { key: '', secret: `01${'00'.repeat(39)}`, apiKeyIndex: '0', applicationId: '718895', testnet: true },
    requests: [{ outputFormat: 'lighter-tx', toSign: auth(718895), expected: /^0x[0-9a-f]{160}$/ }],
  };
};

const CASES: Partial<Record<ExchangeName, () => SigningCase>> = {
  lighter,
  paradex,
  bybit: () => hmac('bybit'),
  bybitinverse: () => hmac('bybitinverse'),
  bybitspot: () => hmac('bybitspot'),
  bybitdemo: () => hmac('bybitdemo'),
  blofin: () => hmac('blofin'),
  okx: () => hmac('okx'),
  okxdemo: () => hmac('okxdemo'),
  okxeu: () => hmac('okxeu'),
  okxeudemo: () => hmac('okxeudemo'),
  bitget: () => hmac('bitget'),
  bitgetusdc: () => hmac('bitgetusdc'),
  bitgetinverse: () => hmac('bitgetinverse'),
  binance: () => hmac('binance'),
  binancepm: () => hmac('binancepm'),
  binancespot: () => hmac('binancespot'),
  binanceinverse: () => hmac('binanceinverse'),
  bingx: () => hmac('bingx'),
  phemex: () => hmac('phemex'),
  hyperliquid: wallet,
  hyperliquidspot: wallet,
};

let frameFed: ExchangeName[] = [];
beforeAll(async () => {
  setWasmEngineLoader(loadNodeEngine);
  const wasm = (await loadNodeEngine()) as { WasmEngine: { frameFedSupported?: (venue: string) => boolean } };
  frameFed = BETA_VENUES.filter((name) => wasm.WasmEngine.frameFedSupported?.(name) === true);
});
afterEach(() => {
  live.splice(0).forEach((exchange) => exchange.dispose());
  setRustEngineTransport();
});

/** The page engine as `createExchangeFromAccount(account, { workerMirror: true })` builds it. */
const pageEngine = async (name: ExchangeName, options: Partial<ExchangeOptions>, now = STAMP, mirror = true) => {
  const sockets: string[] = [];
  const transport: Transport = {
    now: () => now,
    // Frame-fed start may ask for public metadata; signing never needs it.
    request: () => new Promise(() => {}),
    connect: () => {
      sockets.push('ws');
      throw new Error('a page engine opens no venue socket');
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (id) => clearTimeout(id as ReturnType<typeof setTimeout>),
  };
  setRustEngineTransport(() => transport);
  const exchange = new RustEngineExchange(name, { ...options, ...(mirror ? { workerMirror: true } : {}) } as ExchangeOptions, new DefaultStore());
  live.push(exchange);
  return { exchange, sockets };
};

describe('every frame-fed family signs its COSIGNED sign-reqs on the page engine', () => {
  it('the registry lists frame-fed families, and each has a signing case', () => {
    expect(frameFed).toContain('hyperliquid');
    expect(frameFed.filter((name) => !CASES[name])).toEqual([]);
  });

  it.each(BETA_VENUES)('%s', async (name) => {
    if (!frameFed.includes(name)) return;
    const { options, requests, now } = CASES[name]!();
    const { exchange, sockets } = await pageEngine(name, options, now);
    await expect(exchange.frameFedReady).resolves.toBe(true);
    expect(exchange.isFrameFed).toBe(true);
    await answers(exchange, name, requests);
    expect(sockets).toEqual([]);
  });

  // Ready ahead of a family's frame-fed switch: its case already signs on the keyed device.
  it.each(Object.keys(CASES) as ExchangeName[])('%s case signs before its family is frame-fed', async (name) => {
    if (frameFed.includes(name)) return;
    const { options, requests, now } = CASES[name]!();
    const { exchange } = await pageEngine(name, options, now, false);
    await answers(exchange, name, requests);
  });
});

const answers = async (exchange: RustEngineExchange, name: ExchangeName, requests: SigningCase['requests']) => {
    for (const { outputFormat, toSign, expected } of requests) {
      const answer = await exchange.signWsAuth({
        id: `${name}-${outputFormat}`,
        type: 'ws-auth',
        exchangeName: name,
        wsAuth: { toSign, outputFormat: outputFormat as never },
      });
      expect(answer.error, `${name} ${outputFormat}`).toBeUndefined();
      const signature = outputFormat === 'hyperliquid-address' ? answer.wsAuthSignature?.toLowerCase() : answer.wsAuthSignature;
      if (expected instanceof RegExp) expect(signature, `${name} ${outputFormat}`).toMatch(expected);
      else expect(signature, `${name} ${outputFormat}`).toBe(expected);
    }
};
