import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('./gate.mjs', import.meta.url), 'utf8');
const createOutput = source.slice(source.indexOf('const outDir ='), source.indexOf('const records = tapeRecords;'));

test('a reused gate process directory cannot overwrite retained replay evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'gate-output-'));
  const run = (pid) => vm.runInNewContext(createOutput, {
    process: { pid }, values: { venue: 'okx' },
    mkdirSync: (path, options) => mkdirSync(join(root, path), options),
  });
  try {
    run(7);
    const retained = join(root, 'sdk-rs/harness/out/okx-7/ts-0.json');
    writeFileSync(retained, 'original evidence');
    assert.throws(() => run(7), { code: 'EEXIST' });
    assert.equal(readFileSync(retained, 'utf8'), 'original evidence');
    run(8);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
