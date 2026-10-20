// Record a live public-data tape by running the Rust engine against the venue
// and capturing every HTTP response and websocket frame it receives.
//
//   corepack yarn rust:record --venue binance --seconds 60 \
//     --books BTCUSDT,ETHUSDT,SOLUSDT --out sdk-rs/harness/tapes/binance.jsonl
import { createWriteStream, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
import { EngineHost, nativeTransport } from '../hosts/wasm/js/host.mjs';
import { loadEngine } from './engine.mjs';
import { canonicalUrl } from './tape.mjs';
import { venueOf } from './venues.mjs';
import { Frame } from '../hosts/wasm/js/wire.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    seconds: { type: 'string', default: '60' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
  },
});
const out = values.out ?? `sdk-rs/harness/tapes/${values.venue}.jsonl`;
const venue = venueOf(values.venue);
mkdirSync(dirname(out), { recursive: true });
const tape = createWriteStream(out);
const started = Date.now();
let capturing = true;
const write = (record) => { if (capturing) tape.write(`${JSON.stringify({ t: Date.now() - started, ...record })}\n`); };

write({ type: 'meta', epoch: started, private: false });

const live = nativeTransport();
const connections = new Map();
const closedUrls = new Set();
const recording = {
  ...live,
  connect(url, handlers) {
    const key = canonicalUrl(url);
    const connection = connections.get(key) ?? 0;
    connections.set(key, connection + 1);
    write({ type: 'connect', url, connection, socket: connection, initial: !closedUrls.has(key), channel: handlers.channel });
    let closed = false;
    const closeRecord = (code, local = false) => {
      if (closed) return;
      closed = true;
      closedUrls.add(key);
      write({ type: 'close', url, connection, socket: connection, code, local });
    };
    const socket = live.connect(url, {
      channel: handlers.channel,
      context: handlers.context,
      headers: handlers.headers,
      onOpen: () => {
        write({ type: 'open', url, connection, socket: connection });
        handlers.onOpen();
      },
      onMessage: (text) => {
        frames++;
        write({ type: 'ws', url, connection, socket: connection, text });
        handlers.onMessage(text);
      },
      onClose: (code) => {
        closeRecord(code);
        handlers.onClose(code);
      },
    });
    return {
      send(text) {
        let message;
        try { message = JSON.parse(text); } catch { /* Non-JSON heartbeat. */ }
        if (message && venue.socketRequest?.(message)) write({ type: 'ws-send', url, connection, socket: connection, text });
        socket.send(text);
      },
      close: () => { closeRecord(1000, true); socket.close(); },
    };
  },
  async request(request) {
    const response = await live.request(request);
    const recorded = (body) => write({
      type: 'http', method: request.method, url: request.url,
      ...(request.body === undefined ? {} : { requestBody: request.body }),
      status: response.status, headers: response.headers ?? '', body,
    });
    if (response.body instanceof Uint8Array) {
      recorded(new TextDecoder().decode(response.body));
      return response;
    }
    return { ...response, body: (async function* () {
      const decoder = new TextDecoder();
      let body = '';
      for await (const chunk of response.body) {
        body += decoder.decode(chunk, { stream: true });
        yield chunk;
      }
      recorded(body + decoder.decode());
    })() };
  },
};

const options = { ...venue.options, ...venue.publicOptions, ...venue.recordOptions, publicDataOnly: true };
const { WasmEngine } = await loadEngine(venue.rust, options);
let frames = 0;
let updates = 0;
const seenBooks = new Set();
const host = new EngineHost(
  new WasmEngine(
    venue.rust,
    JSON.stringify(options),
  ),
  recording,
  {
    onFrame: (buffer) => {
      updates++;
      for (const book of new Frame(buffer).books()) seenBooks.add(book.market);
    },
    onLog: (level, message) => console.error(`[${level}] ${message}`),
  },
);
host.start();
for (const symbol of values.books.split(',').filter(Boolean)) {
  host.command({ op: 'subscribeOrderBook', symbol });
}
const flush = setInterval(() => host.flush(), 250);
setTimeout(() => {
  clearInterval(flush);
  host.flush();
  capturing = false;
  host.dispose();
  tape.end(() => {
    console.log(JSON.stringify({ out, seconds: Number(values.seconds), storeUpdates: updates, frames, books: seenBooks.size }));
    process.exit(frames > 0 && seenBooks.size >= values.books.split(',').filter(Boolean).length ? 0 : 1);
  });
}, Number(values.seconds) * 1000);
