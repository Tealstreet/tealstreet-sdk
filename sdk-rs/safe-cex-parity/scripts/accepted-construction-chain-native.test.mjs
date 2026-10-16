// Full selected inventory plus compiler-owned construction policy controls.
// Mutation children stop after the unchanged production construction loop;
// the positive selected run reaches the ordinary production inventory exit.
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
const marker = 'TYPES_CONSTRUCTION_COMPLETE ';
const typesOwners = [
  'safe_cex_types::gateway::GatewayCandleUpdate',
  'safe_cex_types::gateway::GatewayMarketUpdate',
  'safe_cex_types::gateway::GatewayOrderbookUpdate',
  'safe_cex_types::gateway::GatewayTickerUpdate',
  'safe_cex_types::object_map::JsObjectMap',
  'safe_cex_types::subscriber_error::StoreSubscriberError',
  'safe_cex_types::validation_error::AccountValidationError',
];
const expectedTypes = typesOwners.map(name => ({ name, traits: ['core::clone::Clone'] }));
const sorted = values => values.slice().sort((a, b) => a.name.localeCompare(b.name));

if (process.argv.includes('--types-construction-checkpoint')) {
  let rewrites = 0, checkpoints = 0;
  const sentinel = {};
  globalThis.__typesConstructionComplete = result => {
    assert.deepEqual(sorted(result), sorted(expectedTypes), 'seven literal compiler construction owners and exact traits');
    console.log(marker + JSON.stringify(sorted(result)));
    throw sentinel;
  };
  const anchor = `  for (const owner of result.construction) {
    assert.ok(construction[owner.name]?.reason?.trim(), \`\${owner.name}: accepted-set assertion: private-construction Clone/Copy must be classified\`);
    assert.deepEqual(owner.traits, construction[owner.name].traits, \`\${owner.name}: accepted-set assertion: private-construction duplication traits changed\`);
  }`;
  const hook = registerHooks({ load(url, context, next) {
    const loaded = next(url, context);
    if (url !== pathToFileURL(runner).href) return loaded;
    const source = typeof loaded.source === 'string' ? loaded.source : Buffer.from(loaded.source).toString('utf8');
    assert.equal(source.split(anchor).length, 2, 'chain checkpoint requires both unique unchanged production construction assertions');
    rewrites++;
    return { ...loaded, source: source.replace(anchor, anchor + `
  assert.equal(pkg.name, 'safe-cex-types', 'chain checkpoint requires actual compiler package');
  assert.equal(target.name, 'safe_cex_types', 'chain checkpoint requires production library');
  globalThis.__typesConstructionComplete(result.construction);`) };
  } });
  process.argv = [process.execPath, runner, '--inventory', '--package=safe-cex-types'];
  try { await import(pathToFileURL(runner).href); assert.fail('chain checkpoint must stop explicitly'); }
  catch (error) { if (error === sentinel) checkpoints++; else throw error; }
  finally { hook.deregister(); delete globalThis.__typesConstructionComplete; }
  assert.equal(rewrites, 1);
  assert.equal(checkpoints, 1, 'complete compiler construction loop executes exactly once');
} else {
  test('selected candle-cache inventory closes every construction owner and rejects both error policy mutations', { timeout: 1800000 }, () => {
    const original = fs.readFileSync(policy);
    const execute = (expected, assertion) => {
      const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--types-construction-checkpoint'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      const output = `${child.stdout}\n${child.stderr}`;
      assert.doesNotMatch(output, /cargo .*did not complete|could not compile|error\[E\d+\]/, 'compiler failure is not a policy control');
      assert.equal(child.status, expected, output);
      if (expected === 0) {
        const receipts = output.split('\n').filter(line => line.startsWith(marker)).map(line => JSON.parse(line.slice(marker.length)));
        assert.deepEqual(receipts, [sorted(expectedTypes)]);
      } else {
        assert.match(output, assertion);
        assert.ok(!output.includes(marker), 'policy refusal must precede complete-loop receipt');
      }
    };
    try {
      // No loader, scope override or early sentinel on this positive run.
      const child = spawnSync(process.execPath, [runner, '--inventory', '--package=safe-cex-candle-cache'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      assert.equal(child.status, 0, `${child.stdout}\n${child.stderr}`);
      const outputs = [...child.stdout.matchAll(/^accepted-set: \d+ Deserialize owners; inventory (.+)$/gm)];
      assert.equal(outputs.length, 1, 'production inventory reaches its ordinary output exactly once');
      const found = JSON.parse(fs.readFileSync(outputs[0][1], 'utf8'));
      assert.deepEqual(found.map(pkg => `${pkg.package}/${pkg.target.name}`).sort(), [
        'safe-cex-candle-cache/candle-cache-owner',
        'safe-cex-candle-cache/safe_cex_candle_cache',
        'safe-cex-types/safe_cex_types',
      ], 'complete literal selected production target closure');
      assert.deepEqual(sorted(found.flatMap(pkg => pkg.construction)), sorted([
        { name: 'safe_cex_candle_cache::cache::CandleCache', traits: ['core::clone::Clone'] },
        { name: 'safe_cex_candle_cache::native::RedisStore', traits: ['core::clone::Clone'] },
        ...expectedTypes,
      ]), 'nine literal construction capabilities across the selected closure');
      console.log('selected candle-cache full production inventory: process exit 0; three targets; nine construction capabilities');
      execute(0);
      for (const [ident, label] of [
        ['validation_error::AccountValidationError', 'account validation error'],
        ['subscriber_error::StoreSubscriberError', 'store subscriber error'],
      ]) {
        const owner = `safe_cex_types::${ident}`;
        const missing = JSON.parse(original); delete missing[owner];
        fs.writeFileSync(policy, JSON.stringify(missing, null, 2) + '\n');
        execute(1, new RegExp(`${owner}: accepted-set assertion: private-construction Clone/Copy must be classified`));
        console.log(`${label} missing construction classification: named assertion; process exit 1`);
        fs.writeFileSync(policy, original); execute(0);
        const wrong = JSON.parse(original); wrong[owner].traits = ['core::marker::Copy'];
        fs.writeFileSync(policy, JSON.stringify(wrong, null, 2) + '\n');
        execute(1, new RegExp(`${owner}: accepted-set assertion: private-construction duplication traits changed`));
        console.log(`${label} wrong construction trait: named assertion; process exit 1`);
        fs.writeFileSync(policy, original); execute(0);
      }
    } finally { fs.writeFileSync(policy, original); }
  });
}
