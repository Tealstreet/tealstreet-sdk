// Fresh compiler construction-stage evidence for one exact owner. Later source,
// decoder/runtime and dependency checks remain outside this checkpoint.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';
import { registerHooks } from 'node:module';
import test from 'node:test';

const scripts = path.dirname(fileURLToPath(import.meta.url));
const runner = path.join(scripts, 'accepted-set.mjs');
const policy = path.join(scripts, '../accepted-set/construction/safe-cex-types.json');
const owner = 'safe_cex_types::object_map::JsObjectMap';
const scope = 'JsObjectMap construction assertion only; later owners, source scans and runtime probes excluded';
const marker = 'OBJECT_MAP_CONSTRUCTION ';

if (process.argv.includes('--object-map-construction-checkpoint')) {
  let rewrites = 0, checkpoints = 0;
  const sentinel = {};
  globalThis.__objectMapConstructionSentinel = sentinel;
  const anchor = `  for (const owner of result.construction) {
    assert.ok(construction[owner.name]?.reason?.trim(), \`\${owner.name}: accepted-set assertion: private-construction Clone/Copy must be classified\`);
    assert.deepEqual(owner.traits, construction[owner.name].traits, \`\${owner.name}: accepted-set assertion: private-construction duplication traits changed\`);
  }`;
  const hook = registerHooks({ load(url, context, next) {
    const loaded = next(url, context);
    if (url !== pathToFileURL(runner).href) return loaded;
    const source = typeof loaded.source === 'string' ? loaded.source : Buffer.from(loaded.source).toString('utf8');
    assert.equal(source.split(anchor).length, 2, 'object-map checkpoint requires the unique unchanged production construction assertions');
    rewrites++;
    const receipt = `
    if (owner.name === ${JSON.stringify(owner)}) {
      assert.equal(pkg.name, 'safe-cex-types', 'object-map checkpoint requires its actual compiler package');
      assert.equal(target.name, 'safe_cex_types', 'object-map checkpoint requires the production library');
      assert.equal(result.construction.filter(item => item.name === ${JSON.stringify(owner)}).length, 1, 'exactly one actual compiler object-map owner');
      assert.deepEqual(owner.traits, ['core::clone::Clone'], 'actual compiler object-map Clone identity');
      console.log(${JSON.stringify(marker)} + JSON.stringify({ owner: owner.name, traits: owner.traits, scope: ${JSON.stringify(scope)} }));
      throw globalThis.__objectMapConstructionSentinel;
    }
`;
    return { ...loaded, source: source.replace(anchor, anchor.slice(0, -4) + receipt + '  }') };
  } });
  process.argv = [process.execPath, runner, '--inventory', '--package=safe-cex-types'];
  try { await import(pathToFileURL(runner).href); assert.fail('object-map checkpoint must stop explicitly'); }
  catch (error) { if (error === sentinel) checkpoints++; else throw error; }
  finally { hook.deregister(); delete globalThis.__objectMapConstructionSentinel; }
  assert.equal(rewrites, 1);
  assert.equal(checkpoints, 1, 'object-map compiler construction checkpoint executes exactly once');
} else {
  test('object-map Clone classification follows the actual compiler owner and exact traits', { timeout: 1200000 }, () => {
    const original = fs.readFileSync(policy);
    const execute = (expected, assertion) => {
      const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--object-map-construction-checkpoint'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      const output = `${child.stdout}\n${child.stderr}`;
      assert.doesNotMatch(output, /cargo .*did not complete|could not compile|error\[E\d+\]/, 'compiler failure is not a construction control');
      assert.equal(child.status, expected, output);
      if (expected === 0) {
        const receipts = output.split('\n').filter(line => line.startsWith(marker)).map(line => JSON.parse(line.slice(marker.length)));
        assert.deepEqual(receipts, [{ owner, traits: ['core::clone::Clone'], scope }]);
      } else {
        assert.match(output, assertion);
        assert.ok(!output.includes(marker), 'classification must refuse before its success receipt');
      }
    };
    try {
      execute(0);
      const missing = JSON.parse(original); delete missing[owner];
      fs.writeFileSync(policy, JSON.stringify(missing, null, 2) + '\n');
      execute(1, /JsObjectMap: accepted-set assertion: private-construction Clone\/Copy must be classified/);
      console.log('object map missing construction classification: named assertion; process exit 1');
      fs.writeFileSync(policy, original); execute(0);
      const wrong = JSON.parse(original); wrong[owner].traits = ['core::marker::Copy'];
      fs.writeFileSync(policy, JSON.stringify(wrong, null, 2) + '\n');
      execute(1, /JsObjectMap: accepted-set assertion: private-construction duplication traits changed/);
      console.log('object map wrong construction trait: named assertion; process exit 1');
      fs.writeFileSync(policy, original); execute(0);
    } finally { fs.writeFileSync(policy, original); }
  });
}
