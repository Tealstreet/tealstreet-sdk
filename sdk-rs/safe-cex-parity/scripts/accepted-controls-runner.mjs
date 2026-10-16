import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

export function verifyControls(output, registry) {
  const names = Object.values(registry.tests).flat();
  assert.equal(new Set(names).size, names.length, 'accepted controls: duplicate registered test');
  const executed = [...output.matchAll(/^ok \d+ - (.+)$/gm)].map(match => match[1]);
  assert.deepEqual(executed.sort(), [...names].sort(), 'accepted controls: named test inventory changed');
  assert.match(output, /^# fail 0$/m, 'accepted controls: failed test');
  assert.match(output, /^# skipped 0$/m, 'accepted controls: skipped test');
  assert.match(output, /^# todo 0$/m, 'accepted controls: todo test');
  const mutations = [...output.matchAll(/^# (.+): named assertion; process exit 1$/gm)].map(match => match[1]);
  assert.deepEqual(mutations.sort(), [...registry.mutations].sort(), 'accepted controls: named mutation inventory changed');
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const directory = path.dirname(fileURLToPath(import.meta.url));
  const registry = JSON.parse(fs.readFileSync(path.join(directory, 'accepted-control-registry.json'), 'utf8'));
  const files = fs.readdirSync(directory).filter(name => /^accepted-.*\.test\.mjs$/.test(name)).sort();
  assert.deepEqual(files, Object.keys(registry.tests).sort(), 'accepted controls: required test files changed');
  const result = spawnSync(process.execPath, ['--test', '--test-concurrency=1', '--test-reporter=tap', ...files.map(name => path.join(directory, name))], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  process.stdout.write(result.stdout ?? '');
  process.stderr.write(result.stderr ?? '');
  assert.equal(result.status, 0, 'accepted controls: runner failed');
  verifyControls(result.stdout, registry);
  console.log(`accepted controls: ${Object.values(registry.tests).flat().length} named tests; ${registry.mutations.length} named assertion mutations`);
}
