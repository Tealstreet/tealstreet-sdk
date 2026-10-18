import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { deserialize } from 'node:v8';
import { hash } from './recorder.mjs';

test('startup adaptation preserves bytes/timing and rejects changed captured inputs', (t) => {
  const dir = mkdtempSync(join(tmpdir(), 'legacy-startup-proof-'));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const frames = [{ base64: Buffer.from([0, 255, 128]).toString('base64'), binary: true, afterMs: 123.5 }];
  const frameFile = join(dir, 'frames.ws.json');
  const bodyFile = join(dir, 'body.json');
  const reportFile = join(dir, 'report.json');
  const bundleFile = join(dir, 'bundle.v8');
  const body = Buffer.from('{"order_book_details":[]}\n');
  writeFileSync(frameFile, JSON.stringify(frames, null, 2));
  writeFileSync(bodyFile, body);
  const report = {
    exchange: 'lighter',
    at: '2026-10-18T12:00:00Z',
    requests: [
      {
        implementation: 'ts',
        method: 'GET',
        host: 'mainnet.zklighter.elliot.ai',
        path: '/stream',
        fixture: frameFile,
        frameHash: hash(JSON.stringify(frames)),
      },
      {
        implementation: 'ts',
        method: 'GET',
        host: 'mainnet.zklighter.elliot.ai',
        path: '/api/v1/orderBookDetails',
        fixture: bodyFile,
        status: 200,
        responseHash: hash(body),
      },
    ],
  };
  writeFileSync(reportFile, JSON.stringify(report));
  const run = () =>
    spawnSync(
      process.execPath,
      [new URL('./legacy-startup-bundle.mjs', import.meta.url).pathname, reportFile, bundleFile],
      { encoding: 'utf8' },
    );
  const ok = run();
  assert.equal(ok.status, 0, ok.stderr);
  const bundle = deserialize(readFileSync(bundleFile));
  assert.deepEqual(Buffer.from(bundle.rest[0][1].bytes), body);
  assert.deepEqual(bundle.tape.frames, [
    { connection: 0, atMs: 123.5, kind: 'message', base64: frames[0].base64, binary: true },
  ]);
  assert.equal(bundle.tape.durationMs, 123.5);
  assert.equal(bundle.capture.windowMs, 123.5, 'conversion cannot fabricate a 60-second capture');
  writeFileSync(bodyFile, Buffer.from('{"changed":true}'));
  assert.notEqual(run().status, 0, 'changed REST fixture must fail its recorded hash');
  writeFileSync(bodyFile, body);
  frames[0].base64 = Buffer.from([1]).toString('base64');
  writeFileSync(frameFile, JSON.stringify(frames));
  assert.notEqual(run().status, 0, 'changed frame sequence must fail its recorded hash');
});
