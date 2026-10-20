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

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    seconds: { type: 'string', default: '60' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
    'options-env': { type: 'string' },
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
write({ type: 'meta', epoch: started, private: false, source: 'live' });
if (policy.replayOptions) write({ type: 'config', options: policy.replayOptions });
else if (options.endpoints) write({ type: 'config', options });

const live = nativeTransport();
const ordinals = new Map();
const recording = {
  ...live,
  connect(url, handlers) {
    const key = canonicalUrl(url);
    const socket = ordinals.get(key) ?? 0;
    ordinals.set(key, socket + 1);
    const connection = live.connect(url, {
      binary: handlers.binary,
      headers: handlers.headers,
      onOpen: () => {
        write({ type: 'open', url, socket });
        handlers.onOpen();
      },
      onMessage: (text) => {
        frames++;
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
  async request(request) {
    const response = await live.request(request);
    const recorded = (body) => {
      write({
        type: 'http',
        method: request.method,
        url: request.url,
        ...(request.body === undefined ? {} : { requestBody: request.body }),
        status: response.status,
        headers: response.headers ?? '',
        body,
      });
      if (response.status >= 200 && response.status < 300) httpResponses++;
    };
    if (response.body instanceof Uint8Array) {
      recorded(new TextDecoder().decode(response.body));
      return response;
    }
    return {
      ...response,
      body: (async function* () {
        const decoder = new TextDecoder();
        let body = '';
        for await (const chunk of response.body) {
          body += decoder.decode(chunk, { stream: true });
          yield chunk;
        }
        recorded(body + decoder.decode());
      })(),
    };
  },
};

const { WasmEngine } = await loadEngine(venue.rust, options);
let frames = 0;
let httpResponses = 0;
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
      const frame = new Frame(buffer);
      for (const book of frame.books()) seenBooks.add(book.market);
      const status = frame.status();
      if (status) loaded = replayStoreReady(status.loaded, false);
    },
    onLog: (level, message) => policy.log(level, message),
  },
);
host.start();
for (const symbol of values.books.split(',').filter(Boolean)) {
  host.command({ op: 'subscribeOrderBook', symbol });
}
const flush = setInterval(() => host.flush(), 250);
setTimeout(
  () => {
    clearInterval(flush);
    host.flush();
    host.dispose();
    tape.end(() => {
      console.log(
        JSON.stringify({ out, seconds: Number(values.seconds), storeUpdates: updates, frames, httpResponses, books: seenBooks.size }),
      );
      process.exit(!recordingFailed && recordingReady(venue, {
        frames, httpResponses, loaded, books: seenBooks.size,
        requestedBooks: values.books.split(',').filter(Boolean).length,
      }) ? 0 : 1);
    });
  },
  Number(values.seconds) * 1000,
);
