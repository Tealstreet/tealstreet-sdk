import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as fs from 'fs';

vi.mock('fs', () => ({
  existsSync: vi.fn(),
  mkdirSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
}));

vi.mock('os', () => ({
  homedir: vi.fn(() => '/home/testuser'),
}));

vi.mock('crypto', () => ({
  randomUUID: vi.fn(() => 'uuid-1'),
}));

import {
  loadCrossTradeFile,
  saveCrossTradeFile,
  listCrossTradeConfigs,
  listAllCrossTradeConfigs,
  getCrossTradeConfig,
  buildSingleCopierConfig,
  upsertCrossTradeConfig,
  upsertCrossTradeConfigRaw,
  removeCrossTradeConfig,
  tombstoneCrossTradeConfig,
  setCrossTradeConfigEnabled,
  gcTombstonedCrossTradeConfigs,
  getRunnableCrossTradeConfigs,
  TOMBSTONE_TTL_MS,
  type StoredCrossTradeConfig,
} from './cross-trade-store';

const makeRow = (overrides: Partial<StoredCrossTradeConfig> = {}): StoredCrossTradeConfig => ({
  id: 'row-1',
  enabled: true,
  masterAccount: 'master',
  masterSymbol: 'BTCUSDT',
  masterWeight: 1,
  copiers: [
    {
      enabled: true,
      account: 'copier',
      weight: 1,
      execution: 'Market',
      weightMode: 'Absolute',
      symbolMappings: {},
    },
  ],
  updatedAt: 1000,
  ...overrides,
});

describe('cross-trade-store', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('loadCrossTradeFile', () => {
    it('returns empty configs when file missing', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      expect(loadCrossTradeFile()).toEqual({ configs: [] });
    });

    it('parses valid file contents', () => {
      const file = { configs: [makeRow()] };
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify(file));
      expect(loadCrossTradeFile()).toEqual(file);
    });

    it('returns empty configs on invalid JSON', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue('not json');
      expect(loadCrossTradeFile()).toEqual({ configs: [] });
    });

    it('returns empty configs on malformed schema', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ wrong: 'shape' }));
      expect(loadCrossTradeFile()).toEqual({ configs: [] });
    });

    it('creates config dir on read when missing', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      loadCrossTradeFile();
      expect(fs.mkdirSync).toHaveBeenCalledWith('/home/testuser/.tealstreet', { recursive: true });
    });
  });

  describe('saveCrossTradeFile', () => {
    it('writes formatted JSON', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      const file = { configs: [makeRow()] };
      saveCrossTradeFile(file);
      expect(fs.writeFileSync).toHaveBeenCalledWith(
        '/home/testuser/.tealstreet/cross-trade.json',
        JSON.stringify(file, null, 2),
      );
    });
  });

  describe('listCrossTradeConfigs', () => {
    it('filters tombstones', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(
        JSON.stringify({
          configs: [makeRow({ id: 'live' }), makeRow({ id: 'dead', deletedAt: 100 })],
        }),
      );
      const live = listCrossTradeConfigs();
      expect(live.map((c) => c.id)).toEqual(['live']);
    });
  });

  describe('listAllCrossTradeConfigs', () => {
    it('includes tombstones', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(
        JSON.stringify({
          configs: [makeRow({ id: 'live' }), makeRow({ id: 'dead', deletedAt: 100 })],
        }),
      );
      const all = listAllCrossTradeConfigs();
      expect(all.map((c) => c.id)).toEqual(['live', 'dead']);
    });
  });

  describe('getCrossTradeConfig', () => {
    it('returns live config by id', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow({ id: 'x' })] }));
      expect(getCrossTradeConfig('x')?.id).toBe('x');
    });

    it('returns undefined for tombstoned id', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow({ id: 'x', deletedAt: 100 })] }));
      expect(getCrossTradeConfig('x')).toBeUndefined();
    });
  });

  describe('buildSingleCopierConfig', () => {
    it('builds a config with one enabled copier and a uuid id', () => {
      const row = buildSingleCopierConfig({
        masterAccount: 'm',
        copierAccount: 'c',
        masterSymbol: 'BTCUSDT',
        copierExecution: 'Market',
      });
      expect(row.id).toBe('uuid-1');
      expect(row.masterAccount).toBe('m');
      expect(row.masterSymbol).toBe('BTCUSDT');
      expect(row.copiers).toHaveLength(1);
      expect(row.copiers[0]?.account).toBe('c');
      expect(row.copiers[0]?.execution).toBe('Market');
      expect(row.copiers[0]?.enabled).toBe(true);
      expect(row.enabled).toBe(false);
    });

    it('defaults execution to LimitSlippage and weight to 1', () => {
      const row = buildSingleCopierConfig({ masterAccount: 'm', copierAccount: 'c' });
      expect(row.copiers[0]?.execution).toBe('LimitSlippage');
      expect(row.copiers[0]?.weight).toBe(1);
    });
  });

  describe('upsertCrossTradeConfig', () => {
    it('appends new row + stamps updatedAt', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [] }));
      const before = Date.now();
      upsertCrossTradeConfig(makeRow({ id: 'new', updatedAt: 0 }));
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs[0]?.id).toBe('new');
      expect(written.configs[0]?.updatedAt).toBeGreaterThanOrEqual(before);
    });

    it('replaces existing row and clears tombstone (resurrect)', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow({ id: 'x', deletedAt: 100 })] }));
      upsertCrossTradeConfig(makeRow({ id: 'x' }));
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs).toHaveLength(1);
      expect(written.configs[0]?.deletedAt).toBeUndefined();
    });
  });

  describe('upsertCrossTradeConfigRaw', () => {
    it('does NOT stamp updatedAt — trusts wire timestamp', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [] }));
      upsertCrossTradeConfigRaw(makeRow({ id: 'wire', updatedAt: 42 }));
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs[0]?.updatedAt).toBe(42);
    });
  });

  describe('removeCrossTradeConfig', () => {
    it('sets deletedAt + updatedAt, returns true', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow({ id: 'x' })] }));
      const removed = removeCrossTradeConfig('x');
      expect(removed).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs[0]?.deletedAt).toBeGreaterThan(0);
    });

    it('returns false when id not found and no createTombstoneIfMissing', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [] }));
      expect(removeCrossTradeConfig('nope')).toBe(false);
    });

    it('creates synthetic tombstone when opted in', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [] }));
      expect(removeCrossTradeConfig('ghost', { createTombstoneIfMissing: true })).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs[0]?.id).toBe('ghost');
      expect(written.configs[0]?.deletedAt).toBeGreaterThan(0);
    });
  });

  describe('tombstoneCrossTradeConfig', () => {
    it('respects inbound updatedAt for LWW', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow({ id: 'x' })] }));
      tombstoneCrossTradeConfig('x', 9999);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs[0]?.updatedAt).toBe(9999);
    });
  });

  describe('setCrossTradeConfigEnabled', () => {
    it('flips enabled and stamps updatedAt; returns true', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(
        JSON.stringify({ configs: [makeRow({ id: 'x', enabled: false, updatedAt: 1 })] }),
      );
      const before = Date.now();
      const ok = setCrossTradeConfigEnabled('x', true);
      expect(ok).toBe(true);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs[0]?.enabled).toBe(true);
      expect(written.configs[0]?.updatedAt).toBeGreaterThanOrEqual(before);
    });

    it('returns false for tombstoned id', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow({ id: 'x', deletedAt: 100 })] }));
      expect(setCrossTradeConfigEnabled('x', true)).toBe(false);
    });
  });

  describe('gcTombstonedCrossTradeConfigs', () => {
    it('drops tombstones older than TTL, keeps recent', () => {
      const now = Date.now();
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(
        JSON.stringify({
          configs: [
            makeRow({ id: 'old', deletedAt: now - TOMBSTONE_TTL_MS - 1 }),
            makeRow({ id: 'fresh', deletedAt: now - 1000 }),
            makeRow({ id: 'live' }),
          ],
        }),
      );
      const removed = gcTombstonedCrossTradeConfigs();
      expect(removed).toBe(1);
      const writeCall = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      const written = JSON.parse(writeCall![1] as string) as { configs: StoredCrossTradeConfig[] };
      expect(written.configs.map((c) => c.id).sort()).toEqual(['fresh', 'live']);
    });

    it('no-op when no expired tombstones', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ configs: [makeRow()] }));
      expect(gcTombstonedCrossTradeConfigs()).toBe(0);
    });
  });

  describe('getRunnableCrossTradeConfigs', () => {
    it('includes only enabled + masterAccount + at least one enabled copier', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(
        JSON.stringify({
          configs: [
            makeRow({ id: 'ok' }),
            makeRow({ id: 'top-disabled', enabled: false }),
            makeRow({ id: 'no-master', masterAccount: undefined }),
            makeRow({
              id: 'no-copier-enabled',
              copiers: [
                {
                  enabled: false,
                  account: 'c',
                  weight: 1,
                  execution: 'Market',
                  weightMode: 'Absolute',
                  symbolMappings: {},
                },
              ],
            }),
            makeRow({ id: 'tombstoned', deletedAt: 1 }),
          ],
        }),
      );
      const runnable = getRunnableCrossTradeConfigs();
      expect(runnable.map((c) => c.id)).toEqual(['ok']);
    });
  });
});
