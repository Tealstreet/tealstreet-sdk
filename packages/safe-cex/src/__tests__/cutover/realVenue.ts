import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';

import { buildBinanceOptionsTape } from '../../../../../sdk-rs/harness/private-tape-binanceoptions.mjs';
import { buildBitgetUtaTape } from '../../../../../sdk-rs/harness/private-tape-bitget-uta.mjs';
import { buildBitgetInverseTape } from '../../../../../sdk-rs/harness/private-tape-bitgetinverse.mjs';
import { coinbasePrivateTape } from '../../../../../sdk-rs/harness/private-tape-coinbase.mjs';
import { canonicalUrl, Tape } from '../../../../../sdk-rs/harness/tape.mjs';
import { venueOf } from '../../../../../sdk-rs/harness/venues.mjs';
import { HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL } from '../../exchanges/bybit/bybit.types';
import { directTransport } from '../../transport/direct';
import { prepareRecordedHttp } from '../support/managedHttp';

type RecordRow = {
  type: string;
  url?: string;
  text?: string;
  binary?: string;
  method?: string;
  body?: string;
  options?: { endpoints: { rest: string; publicWs: string; privateWs: string } };
  [key: string]: unknown;
};

type PredictionInput = {
  conditionId: string;
  slug: string;
  question: string;
  description: string;
  endDate: string;
  clobTokenIds: string;
  outcomes: string;
  orderPriceMinTickSize: number;
  negRisk: boolean;
};

type GrvtInstrumentInput = { i: string; b: string; q: string; k: string; ms: string; ts: string };

type SpotMarketIdentity = { id: string; symbol: string; base: string; quote: string };
type HyperSpotMetaInput = {
  universe: { name: string; tokens: number[] }[];
  tokens: { name: string; index: number }[];
};
type KrakenSpotPairInput = { wsname?: string };

type BreakoutQuote = { symbol: string; bid: number; ask: number };

type BinanceInstrumentInput = {
  symbol: string;
  baseAsset: string;
  quoteAsset: string;
  status?: string;
  contractStatus?: string;
  contractType?: string;
  marginAsset?: string;
  contractSize?: number;
};

const orderlyAliases = new Set(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi']);
const txflowPublicCorsProxy = 'https://txflow-cutover-cors.invalid';

/** Test input only: no network, no SDK store writes and no fabricated empty replies. */
export const realVenue = (name: ExchangeName) => {
  const fixture = orderlyAliases.has(name)
    ? 'orderly'
    : name.startsWith('okxeu')
      ? 'okxeu-xperp'
      : name.startsWith('okx')
        ? 'okx'
        : name === 'binancepm'
          ? 'binance'
          : name === 'coinbasespot'
            ? 'coinbase'
            : name;
  let records: RecordRow[];
  let source: string;
  if (name === 'bitgetinverse' || name === 'bitgetusdc') {
    // These documented schema builders are already inputs to the venue gates.
    // Their public contracts/tickers preserve the actual inverse/USDC product,
    // instead of relabeling the captured USDT payload as another product.
    records = (name === 'bitgetinverse' ? buildBitgetInverseTape() : buildBitgetUtaTape({ usdc: true })) as RecordRow[];
    source =
      name === 'bitgetinverse'
        ? 'private-tape-bitgetinverse.mjs public records'
        : 'private-tape-bitget-uta.mjs USDC public records';
  } else if (name === 'binanceoptions') {
    // Options has no captured public tape; its replay input is the retained TS mapper fixtures.
    records = buildBinanceOptionsTape({ isPublic: true }) as RecordRow[];
    source = 'private-tape-binanceoptions.mjs public records';
  } else if (name === 'coinbase') {
    // Main Coinbase keyless mode is legacy INTX. The captured public tape is
    // spot-only, so use the documented INTX instruments input, with no keys.
    // This proves compatibility, not availability of the retired live route.
    records = coinbasePrivateTape({ profile: 'intx' }).records as RecordRow[];
    source = 'private-tape-coinbase.mjs legacy INTX public records';
  } else {
    const file = new URL(`../../../../../sdk-rs/harness/fixtures/${fixture}-public.jsonl.gz`, import.meta.url);
    records = gunzipSync(readFileSync(file))
      .toString()
      .trim()
      .split('\n')
      .map((line) => JSON.parse(line));
    source = `${fixture}-public.jsonl.gz`;
    if (name === 'breakoutprop') source += ' (documented-scripted DXtrade session, not a live recording)';
  }
  // Never replay commands or a private account payload from a schema builder.
  records = records.filter((record) => record.type === 'http' || record.type === 'ws' || record.type === 'config');
  if (name === 'bitgetinverse' || name === 'bitgetusdc')
    records = records.filter((record) =>
      record.type === 'http'
        ? record.url!.includes('/mix/market/')
        : record.type === 'ws' && record.url!.endsWith('/ws/public'),
    );
  if (name === 'coinbase')
    records = records.filter((record) =>
      record.type === 'http'
        ? record.url!.includes('api.international.coinbase.com')
        : record.type === 'ws' && record.url === 'wss://advanced-trade-ws.coinbase.com',
    );

  const venue = venueOf(name === 'binancepm' ? 'binance' : name);
  if (name === 'kraken' || name === 'krakenspot') venue.initializeReplay('rust');
  const options: ExchangeOptions = {
    key: '',
    secret: '',
    publicDataOnly: true,
    engine: 'rust',
    ...(name === 'txflow'
      ? { testnet: true, transport: { ...directTransport, publicCorsProxy: () => txflowPublicCorsProxy } }
      : {}),
    ...(orderlyAliases.has(name) ? { applicationId: venue.publicOptions.applicationId } : {}),
    ...(name === 'polymarket' ? { polymarket: venue.publicOptions.polymarket } : {}),
    ...(name === 'breakoutprop' ? { ...venue.publicOptions, subaccount: venue.options.subaccount } : {}),
    ...(name === 'hyrotrader' ? { extra: { hyrotraderMode: 'demo' } } : {}),
  };
  const restore: (() => void)[] = [];
  if (name === 'hyrotrader') {
    const endpoints = records.find((record) => record.type === 'config')!.options!.endpoints;
    for (const [object, value] of [
      [HYROTRADER_SIMULATOR_URL, endpoints.rest],
      [HYROTRADER_SIMULATOR_WS_URL.public.linear, endpoints.publicWs],
      [HYROTRADER_SIMULATOR_WS_URL.private, endpoints.privateWs],
    ] as const) {
      const previous = { ...object };
      object.livenet = value;
      object.testnet = value;
      restore.push(() => Object.assign(object, previous));
    }
  }
  // EU captures contain the additional native FUTURES catalog and ticker seed.
  // Demo aliases share their regional product input at their own native hosts.
  if (name.startsWith('okx'))
    records = records.map((record) => {
      if (!record.url) return record;
      const url = new URL(record.url);
      url.hostname =
        url.protocol === 'wss:'
          ? name === 'okxeu'
            ? 'wseea.okx.com'
            : name === 'okxeudemo'
              ? 'wseeapap.okx.com'
              : name === 'okxdemo'
                ? 'wspap.okx.com'
                : 'ws.okx.com'
          : name.startsWith('okxeu')
            ? 'eea.okx.com'
            : name === 'okxdemo'
              ? 'openapi.okx.com'
              : 'www.okx.com';
      return { ...record, url: url.toString() };
    });
  const tape = new Tape(records, venue, false, 2000, 'rust', [], options);
  const polling = venue.polling === true;
  let pollingEpoch: number | undefined;
  const publicPollResponses: Array<{ status: number; quotes: BreakoutQuote[] }> = [];
  const breakoutQuotes: BreakoutQuote[][] =
    name === 'breakoutprop'
      ? records
          .filter((row) => row.type === 'http' && new URL(row.url!).pathname.endsWith('/marketdata'))
          .map((row) =>
            (JSON.parse(row.body!).events as BreakoutQuote[]).map(({ symbol, bid, ask }) => ({ symbol, bid, ask })),
          )
      : [];
  // Spot pair IDs and COIN-M settlement/contract sizes come directly from
  // their distinct recorded exchangeInfo endpoints, not the engine output.
  const binanceMarkets =
    name === 'binancespot' || name === 'binanceinverse'
      ? records
          .filter((row) => row.type === 'http' && new URL(row.url!).pathname.endsWith('/exchangeInfo'))
          .flatMap((row) =>
            (JSON.parse(row.body!).symbols as BinanceInstrumentInput[])
              .filter(
                (instrument) =>
                  name === 'binancespot' ||
                  (instrument.contractType === 'PERPETUAL' && instrument.contractStatus === 'TRADING'),
              )
              .map((instrument) => ({
                id:
                  name === 'binancespot'
                    ? `${instrument.baseAsset}/${instrument.quoteAsset}`
                    : `${instrument.baseAsset}/${instrument.quoteAsset}:${instrument.marginAsset}`,
                symbol: instrument.symbol,
                base: instrument.baseAsset,
                quote: instrument.quoteAsset,
                active: (instrument.status ?? instrument.contractStatus) === 'TRADING',
                ...(name === 'binanceinverse' ? { contractSize: instrument.contractSize } : {}),
              })),
          )
      : [];
  const spotMarkets: SpotMarketIdentity[] = [];
  if (name === 'hyperliquidspot') {
    const row = records.find((record) => record.type === 'http' && record.method === 'POST' &&
      (record.requestBody as string | undefined)?.includes('spotMetaAndAssetCtxs'));
    if (!row) throw new Error('Hyperliquid spot fixture lacks native spot metadata');
    const [meta, contexts] = JSON.parse(row.body!) as [HyperSpotMetaInput, { coin?: string; midPx?: string }[]];
    const positional = contexts.length === meta.universe.length && contexts.every((context) => !context.coin);
    for (const [index, instrument] of meta.universe.entries()) {
      const base = meta.tokens.find((token) => token.index === instrument.tokens[0])?.name;
      const quote = meta.tokens.find((token) => token.index === instrument.tokens[1])?.name;
      const context = contexts.find((context) => context.coin === instrument.name) ??
        (positional ? contexts[index] : undefined);
      if (base && quote && context?.midPx)
        spotMarkets.push({ id: instrument.name, symbol: `${base}/${quote}`, base, quote });
    }
  }
  if (name === 'krakenspot') {
    const row = records.find((record) => record.type === 'http' && new URL(record.url!).pathname === '/0/public/AssetPairs');
    if (!row) throw new Error('Kraken spot fixture lacks native AssetPairs');
    const pairs = (JSON.parse(row.body!) as { result: Record<string, KrakenSpotPairInput> }).result;
    const aliases: Record<string, string> = {
      XBT: 'BTC', XXBT: 'BTC', XDG: 'DOGE', XXDG: 'DOGE', XETH: 'ETH', XXRP: 'XRP', XLTC: 'LTC', XXLM: 'XLM',
      XXMR: 'XMR', XZEC: 'ZEC', XREP: 'REP', XETC: 'ETC', XDAO: 'DAO', XMLN: 'MLN',
      ZUSD: 'USD', ZEUR: 'EUR', ZGBP: 'GBP', ZJPY: 'JPY', ZCAD: 'CAD', ZAUD: 'AUD',
    };
    const asset = (code: string) => aliases[code] ?? code;
    for (const [native, pair] of Object.entries(pairs)) {
      if (native.endsWith('.d') || !pair.wsname) continue;
      const [rawBase, rawQuote] = pair.wsname.split('/');
      const base = asset(rawBase);
      const quote = asset(rawQuote);
      if (['USD', 'USDT', 'USDC'].includes(quote))
        spotMarkets.push({ id: `${base}/${quote}`, symbol: pair.wsname, base, quote });
    }
  }
  // GRVT's Lite REST instruments retain their native mixed-case ID while
  // the SDK symbol is uppercase. Keep the precision oracle in raw API units.
  const grvtMarkets =
    name === 'grvt'
      ? records
          .filter((row) => row.type === 'http' && new URL(row.url!).pathname === '/lite/v1/all_instruments')
          .flatMap((row) =>
            (JSON.parse(row.body!).r as GrvtInstrumentInput[])
              .filter((instrument) => instrument.k === 'PERPETUAL')
              .map((instrument) => ({
                id: instrument.i,
                symbol: instrument.i.toUpperCase(),
                base: instrument.b,
                quote: instrument.q,
                precision: { amount: Number(instrument.ms), price: Number(instrument.ts) },
              })),
          )
      : [];
  // Derive the prediction oracle from raw recorded Gamma fields, independently
  // of the Rust model, frame writer and SDK FrameApplier.
  const predictionMarkets =
    name === 'polymarket'
      ? records
          .filter((row) => row.type === 'http' && new URL(row.url!).hostname === 'gamma-api.polymarket.com')
          .flatMap((row) =>
            (JSON.parse(row.body!).markets as PredictionInput[]).flatMap((market) => {
              const tokens = JSON.parse(market.clobTokenIds) as string[];
              return (JSON.parse(market.outcomes) as string[]).map((outcome, index) => ({
                id: tokens[index],
                symbol: `${market.slug}-${outcome}`,
                info: {
                  conditionId: market.conditionId,
                  tokenId: tokens[index],
                  outcome,
                  tickSize: market.orderPriceMinTickSize,
                  negRisk: market.negRisk,
                  question: market.question,
                  description: market.description,
                  endDate: market.endDate,
                  slug: market.slug,
                },
              }));
            }),
          )
      : [];
  const bookTokens = new Set<string>();
  if (name === 'polymarket')
    for (const record of records.filter((row) => row.type === 'ws')) {
      try {
        const decoded = JSON.parse(record.text!);
        for (const event of Array.isArray(decoded) ? decoded : [decoded])
          if (event.event_type === 'book') bookTokens.add(event.asset_id);
      } catch {
        /* The fixture also contains plain PONG heartbeats. */
      }
    }
  const publicBookSymbols = predictionMarkets
    .filter((market) => bookTokens.has(market.id))
    .map((market) => market.symbol);
  const requests: string[] = [];
  const errors: string[] = [];
  const connectedSocketUrls: string[] = [];
  const canonicalConnectedSocketUrls = new Set<string>();
  const fixtureSocketUrls = new Set(
    records.filter((record) => record.type === 'ws').map((record) => canonicalUrl(record.url!, tape.callerIds, venue)),
  );
  const replayedSocketUrls = new Set<string>();
  const sockets: Array<{ url: string; ordinal: number; onMessage(data: string | Uint8Array): void; closed: boolean }> =
    [];
  const transport: Transport = {
    now: () => Date.now(),
    prepareHttp: prepareRecordedHttp,
    webSocketHeaders: true,
    connect(url, handlers) {
      connectedSocketUrls.push(url);
      canonicalConnectedSocketUrls.add(canonicalUrl(url, tape.callerIds, venue));
      const socket = tape.connect(
        url,
        handlers.onOpen,
        (data: string | Uint8Array) => {
          if (typeof data === 'string') handlers.onMessage(data);
          else handlers.onBinary?.(data);
        },
        handlers.onClose,
      );
      sockets.push(socket);
      return { send: (text) => tape.send(socket, text), close: () => tape.close(socket) };
    },
    async request({ method, url, body, headers, signal }) {
      signal?.throwIfAborted();
      requests.push(`${method} ${url}`);
      // Polling records become available with real elapsed input time. Start
      // after WASM initialization, at the first actual request, and leave the
      // native engine's timers and store publication untouched.
      if (polling) {
        pollingEpoch ??= Date.now();
        tape.time = Math.max(0, Date.now() - pollingEpoch);
      }
      // Exercise the native public-CORS route, then serve its real venue URL
      // from the captured tape without contacting the test-only proxy.
      const venueUrl = name === 'txflow' && url.startsWith(`${txflowPublicCorsProxy}/`)
        ? url.slice(txflowPublicCorsProxy.length + 1)
        : url;
      const response = tape.respond(method, venueUrl, body, headers);
      if (response.status === 599) errors.push(`Untaped ${method} ${url}${body ? ` ${body}` : ''}`);
      if (polling && method === 'POST' && new URL(url).pathname.endsWith('/marketdata') && response.status === 200)
        publicPollResponses.push({
          status: response.status,
          quotes: (JSON.parse(response.body).events as BreakoutQuote[]).map(({ symbol, bid, ask }) => ({
            symbol,
            bid,
            ask,
          })),
        });
      return { status: response.status, headers: response.headers, body: new TextEncoder().encode(response.body) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return {
    options,
    transport,
    requests,
    errors,
    source,
    polling,
    publicPollResponses,
    breakoutQuotes,
    binanceMarkets,
    spotMarkets,
    predictionMarkets,
    grvtMarkets,
    publicBookSymbols,
    connectedSocketUrls,
    canonicalConnectedSocketUrls,
    fixtureSocketUrls,
    replayedSocketUrls,
    /** Deliver captured input to matching live native sockets; the native
     * parser decides which subscribed channels it consumes. No store seeding. */
    async replayPublicFrames(limit = 512) {
      const deadline = Date.now() + 5000;
      while (!sockets.some((socket) => !socket.closed)) {
        if (Date.now() >= deadline)
          throw new Error(`${name} did not connect: ${errors.join('; ')}; requests=${requests.join('; ')}`);
        await new Promise((resolve) => setTimeout(resolve, 1));
      }
      await new Promise<void>((resolve) => setImmediate(resolve));
      let delivered = 0;
      for (const record of records.filter((row) => row.type === 'ws').slice(0, limit)) {
        // This suite subscribes tickers, not books. A taped response to an
        // unissued Orderly orderbook RPC must not be correlated to a new ID.
        if (orderlyAliases.has(name) && record.text && JSON.parse(record.text).event === 'request') continue;
        const key = canonicalUrl(record.url!, tape.callerIds, venue);
        for (const socket of sockets.filter(
          (socket) =>
            !socket.closed &&
            (record.socket === undefined || record.socket === socket.ordinal) &&
            canonicalUrl(socket.url, tape.callerIds, venue) === key,
        )) {
          const data =
            record.binary === undefined ? record.text! : Uint8Array.from(Buffer.from(record.binary, 'base64'));
          socket.onMessage(data);
          replayedSocketUrls.add(canonicalUrl(socket.url, tape.callerIds, venue));
          delivered++;
        }
        await new Promise<void>((resolve) => setImmediate(resolve));
      }
      return delivered;
    },
    dispose() {
      for (const fn of restore) fn();
    },
  };
};
