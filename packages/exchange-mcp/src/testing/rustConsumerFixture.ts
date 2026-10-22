import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';

import { prepareRecordedHttp } from '@tealstreet/safe-cex/src/__tests__/support/managedHttp';
import { directTransport } from '@tealstreet/safe-cex/src/transport/direct';

/** Offline host routing: the separate routing contract suite covers proxies. */
export const fixtureAccountTransport = {
  ...directTransport,
  name: 'offline-consumer-fixture',
  rest: ({ config }: Parameters<typeof directTransport.rest>[0]) => ({ baseURL: config?.baseURL, proxied: false }),
};

// These public test credentials and documented API examples are the venue gate's
// own inputs. No production network or store mutation participates in this proof.
const require = createRequire(import.meta.url);
const { coinbasePrivateTape } = require('../../../../sdk-rs/harness/private-tape-coinbase.mjs');
const { buildBreakoutTape } = require('../../../../sdk-rs/harness/private-tape-breakoutprop.mjs');
const { Tape, canonicalUrl } = require('../../../../sdk-rs/harness/tape.mjs');
const { venueOf } = require('../../../../sdk-rs/harness/venues.mjs');

type RecordRow = {
  type: string;
  url?: string;
  text?: string;
  options?: { key: string; secret: string };
  [key: string]: unknown;
};
const consumerFixture = (records: RecordRow[], name: string, privateAccount: boolean, advanceResponses = false) => {
  const startedAt = Date.now();
  const venue = venueOf(name);
  const tape = new Tape(records, venue, privateAccount);
  const requests: string[] = [];
  const failures: string[] = [];
  const sockets: string[] = [];
  const connections: Array<{ url: string; closed: boolean; onMessage(text: string): void }> = [];
  const transport: Transport = {
    prepareHttp: prepareRecordedHttp,
    now: () => Date.now(),
    webSocketHeaders: true,
    connect(url, handlers) {
      sockets.push(url);
      const socket = tape.connect(url, handlers.onOpen, handlers.onMessage, handlers.onClose);
      connections.push(socket);
      return { send: (text) => tape.send(socket, text), close: () => tape.close(socket) };
    },
    async request({ method, url, body, headers, signal }) {
      signal?.throwIfAborted();
      requests.push(`${method} ${url}`);
      // Time-gated REST examples follow the real native polling clock.
      if (advanceResponses) tape.time = Date.now() - startedAt;
      const response = tape.respond(method, url, body, headers);
      if (response.status === 599) failures.push(`${method} ${url}`);
      return { status: response.status, headers: response.headers, body: new TextEncoder().encode(response.body) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return {
    options: records.find((row) => row.type === 'config')?.options ?? { key: '', secret: '' },
    transport,
    requests,
    failures,
    sockets,
    async replayPublicFrames() {
      await new Promise<void>((resolve) => setImmediate(resolve));
      let delivered = 0;
      for (const record of records.filter((row) => row.type === 'ws').slice(0, 512)) {
        const key = canonicalUrl(record.url!, tape.callerIds, venue);
        for (const socket of connections.filter(
          (socket) => !socket.closed && canonicalUrl(socket.url, tape.callerIds, venue) === key,
        )) {
          socket.onMessage(record.text!);
          delivered++;
        }
        await new Promise<void>((resolve) => setImmediate(resolve));
      }
      return delivered;
    },
  };
};

export const rustConsumerFixture = () =>
  consumerFixture(coinbasePrivateTape({ profile: 'cfm', venue: 'coinbase' }).records, 'coinbase', true);

export const rustBinancePublicFixture = () => {
  const source = new URL('../../../../sdk-rs/harness/fixtures/binance-public.jsonl.gz', import.meta.url);
  const records = gunzipSync(readFileSync(source))
    .toString()
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line));
  return consumerFixture(records, 'binance', false);
};

export const rustBreakoutFixture = () => consumerFixture(buildBreakoutTape(), 'breakoutprop', true, true);
