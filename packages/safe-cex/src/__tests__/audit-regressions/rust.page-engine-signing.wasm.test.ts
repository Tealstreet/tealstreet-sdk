import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { createHmac } from 'node:crypto';
import { ethers } from 'ethers';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { BETA_VENUES } from '../../exchanges/rust/betaVenues';
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
  /** Sign-reqs as the native server's remote signer sends them, with the expected answer. */
  requests: { outputFormat: string; toSign: string; expected: string }[];
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

const CASES: Partial<Record<ExchangeName, () => SigningCase>> = {
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
const pageEngine = async (name: ExchangeName, options: Partial<ExchangeOptions>) => {
  const sockets: string[] = [];
  const transport: Transport = {
    now: () => STAMP,
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
  const exchange = new RustEngineExchange(name, { ...options, workerMirror: true } as ExchangeOptions, new DefaultStore());
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
    const { options, requests } = CASES[name]!();
    const { exchange, sockets } = await pageEngine(name, options);
    await expect(exchange.frameFedReady).resolves.toBe(true);
    expect(exchange.isFrameFed).toBe(true);
    for (const { outputFormat, toSign, expected } of requests) {
      const answer = await exchange.signWsAuth({
        id: `${name}-${outputFormat}`,
        type: 'ws-auth',
        exchangeName: name,
        wsAuth: { toSign, outputFormat: outputFormat as never },
      });
      expect(answer.error, `${name} ${outputFormat}`).toBeUndefined();
      const signature = outputFormat === 'hyperliquid-address' ? answer.wsAuthSignature?.toLowerCase() : answer.wsAuthSignature;
      expect(signature, `${name} ${outputFormat}`).toBe(expected);
    }
    expect(sockets).toEqual([]);
  });
});
