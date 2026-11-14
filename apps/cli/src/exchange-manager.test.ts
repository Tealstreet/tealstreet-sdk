import { afterEach, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { CLIPlatformAdapter, ExchangeAccountConfig, ExchangeLike } from '@tealstreet/cli-core';
const dir = mkdtempSync(join(tmpdir(), 'cli-exchange-owner-'));
vi.stubEnv('TEALSTREET_CONFIG_DIR', dir);
vi.stubEnv('TEALSTREET_CONFIG_FILE', join(dir, 'config.json'));
vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));
afterEach(() => {
  vi.unstubAllEnvs();
  rmSync(dir, { recursive: true, force: true });
});
const account: ExchangeAccountConfig = { name: 'Main', exchange: 'bybit', apiKey: 'fixture', apiSecret: 'fixture' };
const makeExchange = (name: string): ExchangeLike => ({ name, emitter: { on() {}, off() {}, emit() {} } });
it('disposes A, never returns A for B, and rejects an in-flight A creation', async () => {
  const { loadConfig, saveConfig } = await import('./config');
  const { ExchangeManager } = await import('./exchange-manager');
  const login = (userId: string) => {
    const c = loadConfig();
    c.auth = { userId, accessToken: 'fixture' };
    saveConfig(c);
  };
  const a = makeExchange('A'),
    b = makeExchange('B');
  let finish: (value: ExchangeLike) => void = () => {};
  const adapter: Pick<CLIPlatformAdapter, 'createExchange' | 'startExchange' | 'stopExchange' | 'updateAccount'> = {
    createExchange: vi
      .fn()
      .mockResolvedValueOnce(a)
      .mockResolvedValueOnce(b)
      .mockImplementationOnce(
        () =>
          new Promise<ExchangeLike>((resolve) => {
            finish = resolve;
          }),
      ),
    startExchange: vi.fn(),
    stopExchange: vi.fn(),
    updateAccount: vi.fn(),
  };
  login('A');
  const manager = new ExchangeManager(adapter);
  expect(await manager.enable(account)).toBe(a);
  login('B');
  expect(manager.size).toBe(0);
  expect(adapter.stopExchange).toHaveBeenCalledWith(a);
  expect(await manager.enable(account)).toBe(b);
  const pending = manager.enable({ ...account, name: 'Pending' });
  login('A');
  finish(makeExchange('old-B'));
  await expect(pending).rejects.toThrow('login changed');
  expect(manager.get('Pending')).toBeUndefined();
  await manager.stopAll();
});
