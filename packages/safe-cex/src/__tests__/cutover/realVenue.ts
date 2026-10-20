import type { Transport } from '@tealstreet/safe-cex-wasm/host';
import type { ExchangeName, ExchangeOptions } from '../../types';

import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';

import { buildBitgetUtaTape } from '../../../../../sdk-rs/harness/private-tape-bitget-uta.mjs';
import { buildBitgetInverseTape } from '../../../../../sdk-rs/harness/private-tape-bitgetinverse.mjs';
import { coinbasePrivateTape } from '../../../../../sdk-rs/harness/private-tape-coinbase.mjs';
import { canonicalUrl, Tape } from '../../../../../sdk-rs/harness/tape.mjs';
import { venueOf } from '../../../../../sdk-rs/harness/venues.mjs';
import { HYROTRADER_SIMULATOR_URL, HYROTRADER_SIMULATOR_WS_URL } from '../../exchanges/bybit/bybit.types';

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

const orderlyAliases = new Set(['tealstreet-dex', 'kodiakfi', 'mode', 'salsadex', 'what', 'woofi']);

/** Test input only: no network, no SDK store writes and no fabricated empty replies. */
export const realVenue = (name: ExchangeName) => {
  const fixture = orderlyAliases.has(name)
    ? 'orderly'
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
  if (name === 'kraken') venue.initializeReplay('rust');
  const options: ExchangeOptions = {
    key: '',
    secret: '',
    publicDataOnly: true,
    engine: 'rust',
    ...(orderlyAliases.has(name) ? { applicationId: venue.publicOptions.applicationId } : {}),
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
  // EU OKX has identical documented public paths/data, at its regional host.
  if (name.startsWith('okx'))
    records = records.map((record) => ({
      ...record,
      ...(record.url
        ? {
            url: record.url
              .replace(
                'www.okx.com',
                name === 'okxeu' || name === 'okxeudemo'
                  ? 'eea.okx.com'
                  : name === 'okxdemo'
                    ? 'openapi.okx.com'
                    : 'www.okx.com',
              )
              .replace(
                'ws.okx.com',
                name === 'okxeu'
                  ? 'wseea.okx.com'
                  : name === 'okxeudemo'
                    ? 'wseeapap.okx.com'
                    : name === 'okxdemo'
                      ? 'wspap.okx.com'
                      : 'ws.okx.com',
              ),
          }
        : {}),
    }));
  const tape = new Tape(records, venue, false, 2000, 'rust', [], options);
  const requests: string[] = [];
  const errors: string[] = [];
  const sockets: Array<{ url: string; ordinal: number; onMessage(data: string | Uint8Array): void; closed: boolean }> =
    [];
  const transport: Transport = {
    now: () => Date.now(),
    webSocketHeaders: true,
    connect(url, handlers) {
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
      const response = tape.respond(method, url, body, headers);
      if (response.status === 599) errors.push(`Untaped ${method} ${url}${body ? ` ${body}` : ''}`);
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
