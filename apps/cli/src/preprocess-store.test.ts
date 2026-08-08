import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as fs from 'fs';
import { join } from 'path';

vi.mock('fs', () => ({
  existsSync: vi.fn(),
  mkdirSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
}));

vi.mock('./logger', () => ({
  logger: {
    warn: vi.fn(),
  },
}));

const CONFIG_DIR = '/tmp/tealstreet-cli-store-test';

type Files = Record<string, string>;

async function makeStore(files: Files = {}) {
  vi.resetModules();
  process.env.TEALSTREET_CONFIG_DIR = CONFIG_DIR;

  vi.mocked(fs.existsSync).mockImplementation((file) => {
    const key = String(file);
    return key === CONFIG_DIR || Object.prototype.hasOwnProperty.call(files, key);
  });
  vi.mocked(fs.readFileSync).mockImplementation((file) => files[String(file)]);

  const { FilePreprocessStore } = await import('./preprocess-store');
  return new FilePreprocessStore();
}

function file(name: string): string {
  return join(CONFIG_DIR, name);
}

/**
 * Every case calls `makeStore`, which does `vi.resetModules()` and then a
 * dynamic `import()` — so each one re-transforms the module graph. That is
 * cheap when this package runs alone and can exceed the 5s default when the
 * whole repo's tasks are running concurrently, which is exactly what CI does.
 * The filesystem is mocked, so this budget is for transform cost, not I/O.
 */
vi.setConfig({ testTimeout: 30_000 });

describe('FilePreprocessStore', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    process.env.TEALSTREET_CONFIG_DIR = CONFIG_DIR;
  });

  afterEach(() => {
    delete process.env.TEALSTREET_CONFIG_DIR;
  });

  it('loads persisted vars, aliases, whitelist, fatfinger limits, and simulation without rewriting files', async () => {
    const store = await makeStore({
      [file('vars.json')]: JSON.stringify({ size: '100$' }),
      [file('aliases.json')]: JSON.stringify({ scalp: 'buy 100$; sell 100$' }),
      [file('whitelist.json')]: JSON.stringify(['btcusdt', 'ETHUSDT']),
      [file('fatfingers.json')]: JSON.stringify({ btcusdt: 2.5, ETHUSDT: '10' }),
      [file('simulation.json')]: JSON.stringify({ on: true }),
    });

    expect(store.getVar('size')).toBe('100$');
    expect(store.getAlias('scalp')).toBe('buy 100$; sell 100$');
    expect(store.getWhitelist()).toEqual(['BTCUSDT', 'ETHUSDT']);
    expect(store.getFatfinger('btcusdt')).toBe(2.5);
    expect(store.getFatfinger('ETHUSDT')).toBe(10);
    expect(store.getSimulation()).toBe(true);
    expect(fs.writeFileSync).not.toHaveBeenCalled();
  });

  it('persists every mutation to the matching preprocess state file', async () => {
    const store = await makeStore();

    store.setVar('entry', '50000');
    store.setAlias('panic', 'cancel; close');
    store.addToWhitelist('btcusdt');
    store.setFatfinger('ethusdt', 4);
    store.setSimulation(true);

    expect(fs.writeFileSync).toHaveBeenCalledWith(
      file('vars.json'),
      JSON.stringify({ entry: '50000' }, null, 2) + '\n',
      'utf-8',
    );
    expect(fs.writeFileSync).toHaveBeenCalledWith(
      file('aliases.json'),
      JSON.stringify({ panic: 'cancel; close' }, null, 2) + '\n',
      'utf-8',
    );
    expect(fs.writeFileSync).toHaveBeenCalledWith(
      file('whitelist.json'),
      JSON.stringify(['BTCUSDT'], null, 2) + '\n',
      'utf-8',
    );
    expect(fs.writeFileSync).toHaveBeenCalledWith(
      file('fatfingers.json'),
      JSON.stringify({ ETHUSDT: 4 }, null, 2) + '\n',
      'utf-8',
    );
    expect(fs.writeFileSync).toHaveBeenCalledWith(
      file('simulation.json'),
      JSON.stringify({ on: true }, null, 2) + '\n',
      'utf-8',
    );
  });

  it('creates the config directory when it does not exist', async () => {
    vi.resetModules();
    process.env.TEALSTREET_CONFIG_DIR = CONFIG_DIR;
    vi.mocked(fs.existsSync).mockReturnValue(false);

    const { FilePreprocessStore } = await import('./preprocess-store');
    new FilePreprocessStore();

    expect(fs.mkdirSync).toHaveBeenCalledWith(CONFIG_DIR, { recursive: true });
  });

  describe('reactive vars are session-scoped (not persisted)', () => {
    it('setLiveVar does NOT write to any file', async () => {
      const store = await makeStore();
      store.setLiveVar('spread', '$ask-$bid');
      expect(store.getLiveVar('spread')).toBe('$ask-$bid');
      expect(fs.writeFileSync).not.toHaveBeenCalled();
    });

    it('unsetLiveVar / unsetAllLiveVars do NOT write to any file', async () => {
      const store = await makeStore();
      store.setLiveVar('a', '$last');
      store.setLiveVar('b', '$mark');
      store.unsetLiveVar('a');
      store.unsetAllLiveVars();
      expect(store.getLiveVars()).toEqual({});
      expect(fs.writeFileSync).not.toHaveBeenCalled();
    });

    it('setLiveVar clears a same-name static binding AND persists the deletion via vars.json', async () => {
      // setVar persists to vars.json. setLiveVar must clear the static
      // binding via the inherited mutex, which goes through unsetVar
      // (overridden here to persist). vars.json is rewritten without
      // the cleared entry; live binding lives only in memory.
      const store = await makeStore();
      store.setVar('x', '42');
      vi.mocked(fs.writeFileSync).mockClear();

      store.setLiveVar('x', '$ask');

      expect(store.getVar('x')).toBeUndefined();
      expect(store.getLiveVar('x')).toBe('$ask');
      expect(fs.writeFileSync).toHaveBeenCalledWith(
        file('vars.json'),
        JSON.stringify({}, null, 2) + '\n',
        'utf-8',
      );
      const writeTargets = vi.mocked(fs.writeFileSync).mock.calls.map((c) => c[0]);
      expect(writeTargets).toEqual([file('vars.json')]);
    });

    it('setVar clears a same-name live binding (one file write, only vars.json)', async () => {
      const store = await makeStore();
      store.setLiveVar('x', '$ask');
      vi.mocked(fs.writeFileSync).mockClear();

      store.setVar('x', '42');

      expect(store.getLiveVar('x')).toBeUndefined();
      expect(store.getVar('x')).toBe('42');
      const writeCalls = vi.mocked(fs.writeFileSync).mock.calls;
      expect(writeCalls.length).toBe(1);
      expect(writeCalls[0][0]).toBe(file('vars.json'));
    });
  });

  describe('tracked vars are session-scoped (not persisted)', () => {
    it('setTrackedVar / updateTrackedVar do NOT write to any file', async () => {
      const store = await makeStore();
      store.setTrackedVar('hi', 'max', '$pnl');
      store.updateTrackedVar('hi', 50);
      expect(store.getTrackedVar('hi')).toEqual({ kind: 'max', source: '$pnl', value: 50 });
      expect(fs.writeFileSync).not.toHaveBeenCalled();
    });

    it('unsetTrackedVar / unsetAllTrackedVars do NOT write to any file', async () => {
      const store = await makeStore();
      store.setTrackedVar('a', 'max', '$pnl');
      store.setTrackedVar('b', 'min', '$mark');
      store.unsetTrackedVar('a');
      store.unsetAllTrackedVars();
      expect(store.getTrackedVars()).toEqual({});
      expect(fs.writeFileSync).not.toHaveBeenCalled();
    });

    it('setTrackedVar clears a same-name static binding AND persists the deletion', async () => {
      const store = await makeStore();
      store.setVar('x', '42');
      vi.mocked(fs.writeFileSync).mockClear();

      store.setTrackedVar('x', 'max', '$pnl');

      expect(store.getVar('x')).toBeUndefined();
      expect(store.getTrackedVar('x')).toMatchObject({ kind: 'max' });
      // vars.json rewritten without x.
      expect(fs.writeFileSync).toHaveBeenCalledWith(
        file('vars.json'),
        JSON.stringify({}, null, 2) + '\n',
        'utf-8',
      );
      const writeTargets = vi.mocked(fs.writeFileSync).mock.calls.map((c) => c[0]);
      expect(writeTargets).toEqual([file('vars.json')]);
    });

    it('setVar clears a same-name tracked binding (no extra file writes for the tracker clear)', async () => {
      const store = await makeStore();
      store.setTrackedVar('x', 'max', '$pnl');
      vi.mocked(fs.writeFileSync).mockClear();

      store.setVar('x', '42');

      expect(store.getTrackedVar('x')).toBeUndefined();
      expect(store.getVar('x')).toBe('42');
      // Only vars.json — the tracker clear is in-memory only.
      const writeCalls = vi.mocked(fs.writeFileSync).mock.calls;
      expect(writeCalls.length).toBe(1);
      expect(writeCalls[0][0]).toBe(file('vars.json'));
    });
  });
});
