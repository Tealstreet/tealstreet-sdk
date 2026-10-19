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

const { values } = parseArgs({
  options: {
    venue: { type: 'string', default: 'binance' },
    seconds: { type: 'string', default: '60' },
    books: { type: 'string', default: 'BTCUSDT,ETHUSDT,SOLUSDT' },
    out: { type: 'string' },
  },
});
const out = values.out ?? `sdk-rs/harness/tapes/${values.venue}.jsonl`;
mkdirSync(dirname(out), { recursive: true });
const tape = createWriteStream(out);
const started = Date.now();
const write = (record) => tape.write(`${JSON.stringify({ t: Date.now() - started, ...record })}\n`);

const live = nativeTransport();
const recording = {
  ...live,
  connect(url, handlers) {
    return live.connect(url, {
      onOpen: () => {
        write({ type: 'open', url });
        handlers.onOpen();
      },
      onMessage: (text) => {
        write({ type: 'ws', url, text });
        handlers.onMessage(text);
      },
      onClose: (code) => {
        write({ type: 'close', url, code });
        handlers.onClose(code);
      },
    });
  },
  async request(request) {
    const response = await live.request(request);
    // A streamed body is gathered so the tape holds it whole.
    let bytes = response.body;
    if (!(bytes instanceof Uint8Array)) {
      const chunks = [];
      for await (const chunk of bytes) chunks.push(chunk);
      bytes = Buffer.concat(chunks);
    }
    const body = new TextDecoder().decode(bytes);
    const requestBody = request.body ? { requestBody: request.body } : {};
    write({ type: 'http', method: request.method, url: request.url, ...requestBody, status: response.status, body });
    return { ...response, body: bytes };
  },
};

const { WasmEngine } = await loadEngine();
let frames = 0;
const host = new EngineHost(new WasmEngine(values.venue, JSON.stringify({ publicDataOnly: false })), recording, {
  onFrame: () => frames++,
  onLog: (level, message) => console.error(`[${level}] ${message}`),
});
host.start();
for (const symbol of values.books.split(',').filter(Boolean)) {
  host.command({ op: 'subscribeOrderBook', symbol });
}
const flush = setInterval(() => host.flush(), 250);
setTimeout(() => {
  clearInterval(flush);
  host.flush();
  host.dispose();
  tape.end(() => {
    console.log(JSON.stringify({ out, seconds: Number(values.seconds), frames }));
    process.exit(0);
  });
}, Number(values.seconds) * 1000);
