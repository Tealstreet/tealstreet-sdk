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
import { configuredOptions, venueOf } from './venues.mjs';
import { canonicalUrl } from './requests.mjs';
import { Frame } from '../hosts/wasm/js/wire.mjs';
import { createRecordingPolicy, createRecordingTimeline, recordingReady } from './polling.mjs';
import { replayStoreReady } from './tape.mjs';
import { recordingHttp } from './record-http.mjs';

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    seconds: { type: 'string', default: '60' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
    'options-env': { type: 'string' },
    'tealstreet-ws-route': { type: 'boolean', default: false },
  },
});
const out = values.out ?? `sdk-rs/harness/tapes/${values.venue}.jsonl`;
const venue = venueOf(values.venue);
let suppliedOptions = {};
if (values['options-env']) {
  try {
    suppliedOptions = JSON.parse(process.env[values['options-env']]);
    if (!suppliedOptions || typeof suppliedOptions !== 'object' || Array.isArray(suppliedOptions)) throw new Error();
  } catch { throw new Error('Recording options environment variable must contain a JSON object'); }
}
if (venue.recordSession === true && !values['options-env'])
  throw new Error('Session-only recording requires explicit credentials through --options-env');
const options = { ...configuredOptions(values.venue), ...venue.publicOptions, ...venue.recordOptions, ...suppliedOptions, publicDataOnly: true };
if (venue.recordSession === true && (!suppliedOptions.key || !suppliedOptions.secret))
  throw new Error('Session-only recording requires explicit username and password in the options environment variable');
const policy = createRecordingPolicy(venue, options, Boolean(values['options-env']));
mkdirSync(dirname(out), { recursive: true });
const tape = createWriteStream(out);
const started = Date.now();
const timeline = createRecordingTimeline(venue);
let recordingFailed = false;
let loaded = false;
const write = (record) => {
  try { return tape.write(`${JSON.stringify(timeline(policy.sanitize({ t: Date.now() - started, ...record }), loaded))}\n`); }
  catch { recordingFailed = true; throw new Error('Recording refused a protocol record that was not replay-safe'); }
};
write({ type: 'meta', epoch: started, private: false, source: 'live', ...(values['tealstreet-ws-route'] ? { transport: 'tealstreet-ws-route' } : {}) });
if (policy.replayOptions) write({ type: 'config', options: policy.replayOptions });
else if (options.endpoints) write({ type: 'config', options });

let live = nativeTransport();
if (values['tealstreet-ws-route']) {
  const [{ createEngineTransport }, { tealstreetTransport }] = await Promise.all([
    import('../../packages/safe-cex/src/transport/engine.ts'),
    import('../../packages/safe-cex/src/transport/tealstreet/tealstreetTransport.ts'),
  ]);
  live = createEngineTransport(values.venue, {
    ...options, transport: tealstreetTransport, tealstreetWsRoute: true,
    tealstreetProxy: false, tealstreetTunnel: false,
  }, live);
}
const ordinals = new Map();
const http = recordingHttp(live, venue, write, (status) => {
  if (status >= 200 && status < 300) httpResponses++;
});
const tickerSymbols = new Set();
const recording = {
  ...live,
  connect(url, handlers) {
    const key = canonicalUrl(url);
    const socket = ordinals.get(key) ?? 0;
    ordinals.set(key, socket + 1);
    const connection = live.connect(url, {
      ...handlers,
      onOpen: () => {
        write({ type: 'open', url, socket });
        handlers.onOpen();
      },
      onMessage: (text) => {
        frames++;
        for (const symbol of venue.recordTickerSymbols?.(text) ?? []) tickerSymbols.add(symbol);
        write({ type: 'ws', url, socket, text });
        handlers.onMessage(text);
      },
      onBinary: (bytes) => {
        frames++;
        write({ type: 'ws', url, socket, binary: Buffer.from(bytes).toString('base64') });
        handlers.onBinary?.(bytes);
      },
      onClose: (code) => {
        write({ type: 'close', url, socket, code });
        handlers.onClose(code);
      },
    });
    return {
      send(text) {
        let message;
        try {
          message = JSON.parse(text);
        } catch {
          /* Non-JSON heartbeat. */
        }
        if (message && venue.socketRequest?.(message)) write({ type: 'ws-send', url, socket, text });
        connection.send(text);
      },
      close: () => connection.close(),
    };
  },
  // Spread once; `prepareHttp` exists only when the live transport has it.
  ...http,
};

const { WasmEngine } = await loadEngine();
let frames = 0;
let httpResponses = 0;
let updates = 0;
let recordingTimer;
let startupTimer;
let startedWindow = false;
const beginWindow = () => {
  if (startedWindow) return;
  startedWindow = true;
  clearTimeout(startupTimer);
  recordingTimer = setTimeout(finish, Number(values.seconds) * 1000);
};
const seenBooks = new Set();
const host = new EngineHost(
  new WasmEngine(
    venue.rust,
    JSON.stringify({ ...options, ...recording.engineOptions }),
  ),
  recording,
  {
    onFrame: (buffer) => {
      updates++;
      const frame = new Frame(buffer);
      for (const book of frame.books()) seenBooks.add(book.market);
      const status = frame.status();
      if (status) loaded = replayStoreReady(status.loaded, false);
      if (venue.recordAfterReady && loaded) beginWindow();
    },
    onLog: (level, message) => policy.log(level, message),
  },
);
host.start();
for (const symbol of values.books.split(',').filter(Boolean)) {
  host.command({ op: 'subscribeOrderBook', symbol });
}
const flush = setInterval(() => host.flush(), 250);
function finish() {
  clearTimeout(recordingTimer);
  clearTimeout(startupTimer);
  clearInterval(flush);
  host.flush();
  host.dispose();
  tape.end(() => {
    console.log(
      JSON.stringify({ out, seconds: Number(values.seconds), storeUpdates: updates, frames, httpResponses, books: seenBooks.size, ...(venue.recordTickerSymbols ? { tickerSymbols: tickerSymbols.size } : {}) }),
    );
    const requested = values.books.split(',').filter(Boolean);
    const tickersReady = !venue.recordTickerSymbols || requested.every((symbol) => tickerSymbols.has(symbol));
    process.exit(!recordingFailed && tickersReady && recordingReady(venue, {
      frames, httpResponses, loaded, books: seenBooks.size, requestedBooks: requested.length,
    }) ? 0 : 1);
  });
}
if (venue.recordAfterReady) startupTimer = setTimeout(finish, 120_000);
else beginWindow();
