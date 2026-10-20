import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

test('Aster crypto loads only for its selected replay, before memory measurement', () => {
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', `
    import assert from 'node:assert/strict';
    import { registerHooks } from 'node:module';
    const loads = [];
    registerHooks({ resolve(specifier, context, next) {
      if (specifier === 'ethers') loads.push(specifier);
      return next(specifier, context);
    } });
    const { venueOf } = await import('./venues.mjs');
    venueOf('lighter');
    venueOf('binance');
    assert.deepEqual(loads, []);
    await venueOf('asterdex').initializeReplay('rust', {});
    assert.equal(loads.length, 1);
  `], { cwd: new URL('.', import.meta.url), encoding: 'utf8', timeout: 15000 });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
});
