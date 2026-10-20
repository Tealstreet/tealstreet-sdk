import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { createRecordingPolicy, createRecordingTimeline, recordingReady } from './polling.mjs';
import { canonicalUrl } from './tape.mjs';

// Exercise the CLI's production writer, transport and exit decision without I/O.
const source = readFileSync(new URL('./record.mjs', import.meta.url), 'utf8');
const writer = source.slice(source.indexOf('let capturing = true;'), source.indexOf('\nconst live ='));
const transport = source.slice(source.indexOf('const connections = new Map();'), source.indexOf('\nconst { WasmEngine }'));
const stopping = source.slice(source.indexOf('const stop = () => {'), source.indexOf('\nconst duration ='));
const recorder = (live, policy) => {
  const records = [], diagnostics = [], exits = [];
  const factory = new Function('live', 'policy', 'createRecordingTimeline', 'recordingReady', 'canonicalUrl', 'tape', 'console', 'process', `
    const venue = { polling: true };
    const options = {}, values = { books: 'BTCUSDT', seconds: '1' }, started = 0, out = 'fixture.jsonl';
    let loaded = true, httpResponses = 1, frames = 1, updates = 1, host;
    const seenBooks = new Set([0]), flush = undefined, timeline = createRecordingTimeline(venue);
    ${writer}
    ${transport}
    ${stopping}
    return { recording, write, stop, bind: (value) => { host = value; } };
  `);
  const result = factory({ now: () => 0, ...live }, policy, createRecordingTimeline, recordingReady, canonicalUrl,
    { write: (text) => records.push(JSON.parse(text)), end: (done) => done() },
    { log: (...args) => diagnostics.push(args.join(' ')) }, { exit: (status) => exits.push(status) });
  const failures = [];
  const engine = {
    httpStart() {}, httpChunk() {}, httpEnd() {}, takeFrame() {},
    httpFailed(_id, message) {
      failures.push(message);
      return JSON.stringify([{ op: 'log', level: 'error', message }]);
    },
  };
  const host = new EngineHost(engine, result.recording, { onLog: (_level, message) => diagnostics.push(message) });
  result.bind(host);
  return { ...result, host, records, diagnostics, exits, failures };
};

test('a post-ready HTTP recording rejection remains fatal after the host catches it', async () => {
  const secret = 'live-recorder-credential';
  for (const streamed of [false, true]) {
    for (const rejection of ['sanitizer', 'serialization']) {
      const policy = createRecordingPolicy({
        recordSession: true, recordReplayOptions: () => ({}),
        sanitizeRecordingRecord: (record) => record,
      }, { key: secret });
      if (rejection === 'serialization') {
        const sanitize = policy.sanitize;
        policy.sanitize = (record) => {
          const safe = sanitize(record);
          if (record.type === 'http') safe.toJSON = () => { throw new Error(`unsafe serializer: ${secret}`); };
          return safe;
        };
      }
      const body = new TextEncoder().encode(JSON.stringify({ echo: rejection === 'sanitizer' ? secret : 'safe' }));
      const probe = recorder({ request: async () => ({ status: 200, body: streamed ? (async function* () { yield body; })() : body }) }, policy);
      await probe.host.request({ request: 1, method: 'GET', url: 'https://example.test/late-read', headers: [] });
      assert.equal(probe.failures.length, 1, 'EngineHost catches the recording rejection');
      assert.equal(probe.failures[0], 'Recording refused a protocol record that was not replay-safe');
      assert.equal(probe.host.requests.size, 0);
      probe.write({ type: 'ws', text: 'later safe record' });
      probe.stop();
      assert.deepEqual(probe.exits, [1], `${rejection}, streamed=${streamed}`);
      assert.ok(probe.records.some((record) => record.text === 'later safe record'));
      assert.ok(!probe.records.some((record) => record.type === 'http'));
      assert.ok(!JSON.stringify([probe.records, probe.diagnostics, probe.failures]).includes(secret));
    }
  }
});

test('a ready recorder with safe HTTP records still exits successfully', async () => {
  const body = new TextEncoder().encode('{"price":"100"}');
  const probe = recorder({ request: async () => ({ status: 200, body }) }, createRecordingPolicy({}, {}));
  await probe.host.request({ request: 1, method: 'GET', url: 'https://example.test/late-read', headers: [] });
  probe.stop();
  assert.deepEqual(probe.failures, []);
  assert.deepEqual(probe.exits, [0]);
  assert.equal(probe.records.find((record) => record.type === 'http').body, '{"price":"100"}');
});
