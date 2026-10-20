import type { Transport } from '@tealstreet/safe-cex-wasm/host';

import { createRequire } from 'node:module';

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
const { Tape } = require('../../../../sdk-rs/harness/tape.mjs');
const { venueOf } = require('../../../../sdk-rs/harness/venues.mjs');

export const rustConsumerFixture = () => {
  const input = coinbasePrivateTape({ profile: 'cfm', venue: 'coinbase' });
  const tape = new Tape(input.records, venueOf('coinbase'), true);
  const requests: string[] = [];
  const failures: string[] = [];
  const sockets: string[] = [];
  const transport: Transport = {
    now: () => Date.now(),
    webSocketHeaders: true,
    connect(url, handlers) {
      sockets.push(url);
      const socket = tape.connect(url, handlers.onOpen, handlers.onMessage, handlers.onClose);
      return { send: (text) => tape.send(socket, text), close: () => tape.close(socket) };
    },
    async request({ method, url, body, headers, signal }) {
      signal?.throwIfAborted();
      requests.push(`${method} ${url}`);
      const response = tape.respond(method, url, body, headers);
      if (response.status === 599) failures.push(`${method} ${url}`);
      return { status: response.status, headers: response.headers, body: new TextEncoder().encode(response.body) };
    },
    setTimeout: (fn, ms) => setTimeout(fn, ms),
    clearTimeout: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
  };
  return {
    options: input.records.find((row: { type: string }) => row.type === 'config').options,
    transport,
    requests,
    failures,
    sockets,
  };
};
