import { describe, it, expect, vi, beforeEach } from 'vitest';
import * as fs from 'fs';

vi.mock('fs', () => ({
  existsSync: vi.fn(),
  mkdirSync: vi.fn(),
  readFileSync: vi.fn(),
  writeFileSync: vi.fn(),
  unlinkSync: vi.fn(),
}));

vi.mock('os', () => ({
  homedir: vi.fn(() => '/home/testuser'),
}));

import {
  writeHeartbeat,
  clearHeartbeat,
  readHeartbeat,
  readFreshHeartbeat,
  getHeartbeatFilePath,
  STALE_MS,
  type CrossTradeHeartbeat,
} from './cross-trade-heartbeat';

const makeHb = (overrides: Partial<CrossTradeHeartbeat> = {}): CrossTradeHeartbeat => ({
  version: 1,
  pid: 1234,
  startedAt: 1000,
  configCount: 2,
  lastHeartbeatAt: 1000,
  ...overrides,
});

describe('cross-trade-heartbeat', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('writeHeartbeat', () => {
    it('writes JSON to ~/.tealstreet/cross-trade-running.json', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      writeHeartbeat({ startedAt: 5000, configCount: 3 });
      const call = vi.mocked(fs.writeFileSync).mock.calls.at(-1);
      expect(call![0]).toBe('/home/testuser/.tealstreet/cross-trade-running.json');
      const written = JSON.parse(call![1] as string) as CrossTradeHeartbeat;
      expect(written.version).toBe(1);
      expect(written.startedAt).toBe(5000);
      expect(written.configCount).toBe(3);
      expect(written.pid).toBe(process.pid);
      expect(written.lastHeartbeatAt).toBeGreaterThan(0);
    });

    it('mkdir -p on the config dir when missing', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      writeHeartbeat({ startedAt: 0, configCount: 0 });
      expect(fs.mkdirSync).toHaveBeenCalledWith('/home/testuser/.tealstreet', { recursive: true });
    });
  });

  describe('clearHeartbeat', () => {
    it('unlinks the file when present', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      clearHeartbeat();
      expect(fs.unlinkSync).toHaveBeenCalledWith('/home/testuser/.tealstreet/cross-trade-running.json');
    });

    it('no-op when file absent', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      clearHeartbeat();
      expect(fs.unlinkSync).not.toHaveBeenCalled();
    });

    it('swallows unlink errors', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.unlinkSync).mockImplementation(() => {
        throw new Error('EACCES');
      });
      expect(() => clearHeartbeat()).not.toThrow();
    });
  });

  describe('readHeartbeat', () => {
    it('returns null when file missing', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      expect(readHeartbeat()).toBeNull();
    });

    it('returns parsed shape when valid', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify(makeHb()));
      expect(readHeartbeat()).toEqual(makeHb());
    });

    it('returns null on invalid JSON', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue('not json');
      expect(readHeartbeat()).toBeNull();
    });

    it('returns null on wrong version', () => {
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify({ ...makeHb(), version: 2 }));
      expect(readHeartbeat()).toBeNull();
    });
  });

  describe('readFreshHeartbeat', () => {
    it('returns the heartbeat when fresh', () => {
      const now = 100_000;
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify(makeHb({ lastHeartbeatAt: now - 1000 })));
      const got = readFreshHeartbeat(now);
      expect(got?.lastHeartbeatAt).toBe(now - 1000);
    });

    it('returns null when heartbeat is stale', () => {
      const now = 100_000;
      vi.mocked(fs.existsSync).mockReturnValue(true);
      vi.mocked(fs.readFileSync).mockReturnValue(JSON.stringify(makeHb({ lastHeartbeatAt: now - STALE_MS - 1 })));
      expect(readFreshHeartbeat(now)).toBeNull();
    });

    it('returns null when file missing', () => {
      vi.mocked(fs.existsSync).mockReturnValue(false);
      expect(readFreshHeartbeat(Date.now())).toBeNull();
    });
  });

  describe('getHeartbeatFilePath', () => {
    it('returns the expected path', () => {
      expect(getHeartbeatFilePath()).toBe('/home/testuser/.tealstreet/cross-trade-running.json');
    });
  });
});
