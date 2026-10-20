import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { EngineHost } from '../hosts/wasm/js/host.mjs';
import { createRecordingPolicy, createRecordingTimeline, recordingReady } from './polling.mjs';
import { canonicalUrl } from './tape.mjs';
import { Frame, LOADED_FLAG, SECTION } from '../hosts/wasm/js/wire.mjs';

// Exercise the CLI's production writer, transport and exit decision without I/O.
const source = readFileSync(new URL('./record.mjs', import.meta.url), 'utf8');
const writer = source.slice(source.indexOf('let capturing = true;'), source.indexOf('\nlet live ='));
const transport = source.slice(source.indexOf('const connections = new Map();'), source.indexOf('\nconst { WasmEngine }'));
const stopping = source.slice(source.indexOf('const stop = () => {'), source.indexOf('\nconst duration ='));
const recorder = (live, policy, settings = {}) => {
  const records = [], diagnostics = [], exits = [];
  const factory = new Function('live', 'policy', 'createRecordingTimeline', 'recordingReady', 'canonicalUrl', 'tape', 'console', 'process', 'settings', `
    const venue = { polling: true, recordAfterReady: settings.afterReady };
    const Date = { now: () => settings.now ?? 2000 };
    const options = {}, values = { books: 'BTCUSDT', seconds: '1' }, started = 0, out = 'fixture.jsonl';
    let loaded = true, httpResponses = 1, frames = 1, updates = 1, host, windowStarted = settings.windowStarted;
    const seenBooks = new Set([0]), flush = undefined, timeline = createRecordingTimeline(venue);
    ${writer}
    ${transport}
    ${stopping}
    return { recording, write, stop, bind: (value) => { host = value; } };
  `);
  const result = factory({ now: () => 0, ...live }, policy, createRecordingTimeline, recordingReady, canonicalUrl,
    { write: (text) => records.push(JSON.parse(text)), end: (done) => { if (settings.endDelay) settings.now += settings.endDelay; done(); } },
    { log: (...args) => diagnostics.push(args.join(' ')) }, { exit: (status) => exits.push(status) }, settings);
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

test('actual recorder frame starts its public recording window from markets and tickers flags', () => {
  const callback = source.slice(source.indexOf('    onFrame: (buffer) => {'), source.indexOf('\n    onLog: policy.log,'));
  const make = new Function('Frame', 'LOADED_FLAG', `
    let updates = 0, loaded = false, windowStarted;
    const seenBooks = new Set(), Date = { now: () => 1234 };
    return { ${callback} state: () => ({loaded, windowStarted}) };
  `);
  const frame = (loaded) => {
    const buffer = new ArrayBuffer(40), view = new DataView(buffer);
    view.setUint32(0, 0x46584353, true);
    view.setUint16(4, 1, true);
    view.setUint16(6, 1, true);
    view.setUint16(16, SECTION.STATUS, true);
    view.setUint32(20, 16, true);
    view.setUint32(24, loaded, true);
    return buffer;
  };
  const recorder = make(Frame, LOADED_FLAG);
  recorder.onFrame(frame(LOADED_FLAG.BALANCE | LOADED_FLAG.ORDERS));
  assert.deepEqual(recorder.state(), {loaded: false, windowStarted: undefined});
  recorder.onFrame(frame(LOADED_FLAG.MARKETS));
  assert.deepEqual(recorder.state(), {loaded: false, windowStarted: undefined});
  recorder.onFrame(frame(LOADED_FLAG.MARKETS | LOADED_FLAG.TICKERS));
  assert.deepEqual(recorder.state(), {loaded: true, windowStarted: 1234});
  recorder.onFrame(frame(0));
  assert.deepEqual(recorder.state(), {loaded: true, windowStarted: 1234}, 'public readiness remains latched');
});

test('readiness deadline cannot report a complete recording window before it actually elapsed', () => {
  for (const [settings, expected] of [
    [{afterReady: true, now: 121000}, 1],
    [{afterReady: true, windowStarted: 120500, now: 121000}, 1],
    [{afterReady: true, windowStarted: 120001, now: 121000, endDelay: 1}, 1],
    [{afterReady: true, windowStarted: 120000, now: 121000}, 0],
    [{afterReady: false, now: 1000}, 0],
  ]) {
    const probe = recorder({}, createRecordingPolicy({}, {}), settings);
    probe.stop();
    assert.deepEqual(probe.exits, [expected]);
  }
});
