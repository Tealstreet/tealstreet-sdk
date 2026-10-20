import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { gzipSync } from 'node:zlib';
import test from 'node:test';
import vm from 'node:vm';
import { acquireReplayFixture } from './replay-fixture.mjs';

const text = '\n' + [
  { type: 'meta', epoch: 123, label: '雪 😀 \ud800' },
  { type: 'ws', text: '{ "id":9007199254740993, "value":"Ä" }\n', effectiveAt: 7 },
  { type: 'http', body: '', effectiveAt: 9, availableAfter: 0 },
  { type: 'command', effectiveAt: null },
].map((row) => JSON.stringify(row)).join('\n\n') + '\n';

test('fixture ownership preserves actual input objects and exact plain/gzip parsing semantics', () => {
  for (const gzip of [false, true]) {
    const bytes = gzip ? gzipSync(text) : Buffer.from(text);
    const lease = acquireReplayFixture(gzip ? 'fixture.gz' : 'fixture.jsonl', { read: () => bytes });
    try {
      assert.equal(lease.inputs.compressedBytes, bytes);
      assert.equal(lease.inputs.decodedText, text);
      assert.deepEqual(lease.inputs.lines, text.split('\n'));
      assert.deepEqual(lease.inputs.filteredLines, text.split('\n').filter(Boolean));
      const expected = text.split('\n').filter(Boolean).map((line) => {
        const record = JSON.parse(line);
        if (record.effectiveAt !== undefined) record.availableAfter ??= record.effectiveAt;
        return record;
      });
      assert.deepEqual(lease.records, expected);
      assert.equal(lease.inputs.records, lease.records);
    } finally { lease.release(); }
    assert.equal(lease.inputs, undefined);
    assert.equal(lease.records, undefined);
    assert.doesNotThrow(() => lease.release());
  }
});

test('read, gzip and JSON errors preserve their original failure', () => {
  const failure = new Error('read failed');
  assert.throws(() => acquireReplayFixture('fixture', { read: () => { throw failure; } }), (error) => error === failure);
  assert.throws(() => acquireReplayFixture('fixture.gz', { read: () => Buffer.from('not gzip') }));
  assert.throws(() => acquireReplayFixture('fixture', { read: () => Buffer.from('{bad JSON') }), SyntaxError);
});

test('real GC retains actual loader roots until release and keeps concurrent leases independent', () => {
  const moduleUrl = new URL('./replay-fixture.mjs', import.meta.url).href;
  const script = `
    import assert from 'node:assert/strict';
    import { acquireReplayFixture } from ${JSON.stringify(moduleUrl)};
    const turn = () => new Promise(setImmediate);
    async function collect() { for (let i=0;i<4;i++) { await turn(); await gc({type:'major',execution:'async'}); } await turn(); }
    function acquire() {
      const lease=acquireReplayFixture('fixture', {read:()=>Buffer.from(''+JSON.stringify({type:'ws',text:'x'.repeat(100000)}))});
      const refs=['compressedBytes','lines','filteredLines','records'].map(key=>new WeakRef(lease.inputs[key]));
      refs.push(new WeakRef(lease.inputs));
      return {lease,refs};
    }
    const first=acquire(),second=acquire();
    await collect();
    for (const ref of [...first.refs,...second.refs]) assert(ref.deref(),'input root lost before initial snapshot');
    await collect();
    for (const ref of [...first.refs,...second.refs]) assert(ref.deref(),'input root lost before final GC/serialization');
    first.lease.release();first.lease.release();
    await collect();
    for (const ref of first.refs) assert.equal(ref.deref(),undefined,'released lease retains its input');
    for (const ref of second.refs) assert(ref.deref(),'releasing one lease destroyed another');
    second.lease.release();
    await collect();
    for (const ref of second.refs) assert.equal(ref.deref(),undefined,'second release retains its input');
    let failedInput;
    assert.throws(()=>acquireReplayFixture('bad',{read:()=>{const bytes=Buffer.from('{bad');failedInput=new WeakRef(bytes);return bytes;}}));
    await collect();assert.equal(failedInput.deref(),undefined,'failed parsing leaked a registered input');
    console.log('PASS real GC fixture lifecycle');
  `;
  const result = spawnSync(process.execPath, ['--expose-gc', '--input-type=module', '-e', script], { encoding: 'utf8', timeout: 10000 });
  assert.equal(result.status, 0, result.stderr || result.error?.message);
  assert.match(result.stdout, /PASS real GC fixture lifecycle/);
});

test('actual replay entry retains inputs through serialization and releases on run/write errors before exit', async () => {
  const source = readFileSync(new URL('./replay.mjs', import.meta.url), 'utf8')
    .replace(/^import .* from .*;\n/gm, '')
    .replaceAll('import.meta.url', JSON.stringify(new URL('./replay.mjs', import.meta.url).href))
    .replace("const result = values.impl === 'ts' ? await runTs() : await runRust();", 'const result = await runImplementation();')
    .replace("const { writeFileSync } = await import('node:fs');", 'const writeFileSync = writeReplayOutput;');
  for (const failAt of [undefined, 'run', 'write']) {
    const events = [];
    const failure = new Error(failAt);
    let lease;
    const context = {
      parseArgs: () => ({ values: { impl: 'rust', venue: 'fixture', tape: 'fixture', books: '', delivery: 'batch', cadence: 'recorded' } }),
      venueOf: () => ({}), deterministicReplay: () => false,
      performance: { now: () => 0 },
      realTimeout: () => ({ unref() {} }), realClearTimeout() {},
      acquireReplayFixture() {
        events.push('acquire');
        lease = acquireReplayFixture('fixture', { read: () => Buffer.from('{"type":"meta"}\n') });
        return { get records() { return lease.records; }, release() { events.push('release'); lease.release(); } };
      },
      async runImplementation() {
        assert(lease.inputs);
        events.push('run');
        if (failAt === 'run') throw failure;
        return { store: { markets: [], tickers: [] }, books: {}, commands: [], writes: [], metrics: {} };
      },
      writeReplayOutput() {
        assert(lease.inputs, 'fixture ownership ended before serialization');
        events.push('write');
        if (failAt === 'write') throw failure;
      },
      process: { exit(code) { assert.equal(code, 0); assert.equal(lease.inputs, undefined); events.push('exit'); } },
    };
    const promise = vm.runInNewContext(`(async () => { ${source} })()`, context);
    if (failAt) await assert.rejects(promise, (error) => error === failure);
    else await promise;
    assert.equal(lease.inputs, undefined);
    assert.deepEqual(events, ['acquire', 'run', ...(failAt === 'run' ? [] : ['write']), 'release', ...(failAt ? [] : ['exit'])]);
  }
});
