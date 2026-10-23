import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { compareRecordedReport, createRecordedReplay, recordedReportFields } from './recorded-replay.mjs';
import { performanceOrder } from './performance-policy.mjs';
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const fixture = (run) => {
  const root = mkdtempSync(join(tmpdir(), 'recorded-gate-'));
  const out = join(root, 'sdk-rs/harness/out'); mkdirSync(out, { recursive: true });
  const tape = 'public.jsonl'; writeFileSync(join(root, tape), '{"type":"meta","private":false}\n');
  const bytes = '{"type":"meta","private":false}\n';
  const slots = [...['ts', 'rust'].map((impl) => ({ impl, mode: 'correctness', run: 0 })),
    ...Array.from({ length: 9 }, (_, index) => performanceOrder(index).map((impl) => ({ impl, mode: 'performance', run: index }))).flat()];
  const raw = slots.map((slot, index) => {
    const bytes = JSON.stringify({ impl: slot.impl, frames: index, metrics: { fixed: true } });
    writeFileSync(join(out, `${index}.json`), bytes);
    return { ...slot, bytes, sha256: hash(bytes) };
  });
  const evidence = { report: { schema: 1, private: false, runs: 9, correctnessRuns: 1, venue: 'mexc', tape,
    tapeSha256: hash(bytes), books: 'BTC', artifacts: slots.map((slot, index) => ({ ...slot, path: join(out, `${index}.json`) })) }, raw };
  try { run({ root, evidence, slots, reader: () => createRecordedReplay({ root, evidence, venue: 'mexc', tape, books: 'BTC' }) }); }
  finally { rmSync(root, { recursive: true, force: true }); }
};

test('recorded reader consumes every exact slot once without a subprocess fallback', () => fixture(({ slots, reader }) => {
  const source = reader(); assert.throws(() => source.finish(), /complete inventory/);
  for (const slot of slots) assert.equal(source.read(slot.impl, slot.run, slot.mode === 'performance').impl, slot.impl);
  source.finish(); assert.throws(() => source.read('ts', 0, false), /no replay fallback/);
}));

test('missing, duplicate, reordered, tampered and substituted case evidence is refused', () => {
  for (const mutate of [
    ({ evidence }) => { evidence.raw.pop(); },
    ({ evidence }) => { evidence.raw[2] = evidence.raw[3]; },
    ({ evidence }) => { evidence.raw[0].bytes += ' '; },
    ({ evidence }) => { evidence.report.artifacts[0] = null; },
    ({ evidence }) => { evidence.report.tapeSha256 = 'f'.repeat(64); },
    ({ evidence }) => { evidence.report.books = 'ETH'; },
    ({ evidence }) => { evidence.report.artifacts[0].path = '../../outside'; },
  ]) fixture((value) => { mutate(value); assert.throws(value.reader); });
});

test('offline strict rederivation compares every original decision and sample field', () => {
  const original = Object.fromEntries(recordedReportFields.map((field) => [field, { fixed: field }]));
  assert.deepEqual(compareRecordedReport(original, structuredClone(original)), []);
  for (const field of recordedReportFields) {
    const changed = structuredClone(original); changed[field] = 'changed';
    assert.deepEqual(compareRecordedReport(original, changed), [`offline strict report changed ${field}`]);
  }
});


test('invalid UTF-8 cannot be replaced before hashing retained raw bytes', () => fixture(({ evidence, reader }) => {
  const original = evidence.raw[0].bytes;
  const bytes = Buffer.concat([Buffer.from(original.slice(0, -1) + ',"bad":"'), Buffer.from([0xff]), Buffer.from('"}')]);
  writeFileSync(evidence.report.artifacts[0].path, bytes);
  evidence.raw[0].bytes = bytes.toString('utf8');
  evidence.raw[0].sha256 = hash(evidence.raw[0].bytes);
  assert.throws(reader, /encoded data|encoding|UTF/i);
}));


test('one physical same-implementation sample cannot substitute for another slot', () => fixture(({ evidence, reader }) => {
  evidence.raw[5].bytes = evidence.raw[2].bytes;
  evidence.raw[5].sha256 = evidence.raw[2].sha256;
  evidence.report.artifacts[5].path = evidence.report.artifacts[2].path;
  assert.throws(reader, /duplicated or substituted/);
}));
