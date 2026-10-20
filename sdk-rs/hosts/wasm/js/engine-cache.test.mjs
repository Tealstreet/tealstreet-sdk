import assert from 'node:assert/strict';
import { test } from 'node:test';
import { venueFamily } from './venues.mjs';

let instance = 0;
const freshCache = async () => import(`./engine-cache.mjs?cache-test=${instance++}`);

test('default and explicit browser paths share aliases, initialization and live memory', async () => {
  const { initializeWebEngine } = await freshCache();
  let imports = 0;
  let initializations = 0;
  let memory;
  let finish;
  const ready = new Promise((resolve) => { finish = resolve; });
  const module = {
    default: async () => { initializations++; await ready; memory = { generation: initializations }; },
    WasmEngine: class { constructor() { this.memory = memory; } },
  };
  // These import callbacks represent independent entry points into the shared
  // module: the host's default loader and the SDK's explicit literal switch.
  const explicit = (venue) => initializeWebEngine(venueFamily(venue), async () => { imports++; return module; });
  const defaultLoader = (venue) => initializeWebEngine(venueFamily(venue), async () => { imports++; return module; });
  const first = explicit('okx');
  const second = defaultLoader('okxeu');
  const third = defaultLoader('okxdemo');
  assert.equal(first, second);
  assert.equal(second, third);
  finish();
  const loaded = await first;
  assert.equal(await second, loaded);
  const live = new loaded.WasmEngine();
  assert.equal(await defaultLoader('okxeudemo'), loaded);
  assert.equal(live.memory, memory);
  assert.deepEqual(live.memory, { generation: 1 });
  assert.equal(imports, 1);
  assert.equal(initializations, 1);
});

test('different families initialize independently', async () => {
  const { initializeWebEngine } = await freshCache();
  const initialized = [];
  const load = (family) => initializeWebEngine(family, async () => ({
    default: async () => { initialized.push(family); },
    WasmEngine: class {},
  }));
  const okx = load('okx');
  const bybit = load('bybit');
  assert.notEqual(okx, bybit);
  const modules = await Promise.all([okx, bybit]);
  assert.notEqual(modules[0], modules[1]);
  assert.deepEqual(initialized.sort(), ['bybit', 'okx']);
});

test('failed imports and initialization can retry without dropping other families', async () => {
  for (const failure of ['import', 'initialize']) {
    const { initializeWebEngine } = await freshCache();
    const bybit = await initializeWebEngine('bybit', async () => ({ default: async () => {}, WasmEngine: class {} }));
    let attempts = 0;
    const load = () => initializeWebEngine('okx', () => {
      attempts++;
      if (attempts === 1 && failure === 'import') throw new Error('download failed');
      return Promise.resolve({
        default: async () => { if (attempts === 1) throw new Error('initialization failed'); },
        WasmEngine: class {},
      });
    });
    const first = load();
    assert.equal(first, load());
    await assert.rejects(first, failure === 'import' ? /download failed/ : /initialization failed/);
    await load();
    assert.equal(attempts, 2);
    assert.equal(await initializeWebEngine('bybit', () => { throw new Error('must stay cached'); }), bybit);
  }
});
