import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { ExchangeAccountConfig } from '@tealstreet/cli-core';

vi.mock('./logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
  originalConsoleLog: vi.fn(),
  originalConsoleError: vi.fn(),
  originalConsoleWarn: vi.fn(),
  writeAbovePrompt: (fn: () => void) => fn(),
  isStdoutReservedForJson: () => false,
}));
const home = vi.hoisted(() => ({ path: '' }));
vi.mock('os', async (original) => ({ ...(await original<typeof import('os')>()), homedir: () => home.path }));

beforeAll(async () => {
  // Transform the shared command/SDK graph outside the timed isolation assertions.
  // Each case still resets modules and creates its own config/store instances.
  await import('@tealstreet/cli-core');
}, 30_000);

let root: string;
beforeEach(() => {
  vi.resetModules();
  root = mkdtempSync(join(tmpdir(), 'cli-owner-test-'));
  home.path = root;
  vi.stubEnv('TEALSTREET_BRAND', 'tealstreet');
  vi.stubEnv('TEALSTREET_CONFIG_DIR', join(root, '.tealstreet'));
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, '.tealstreet/config.json'));
});
afterEach(() => {
  vi.unstubAllEnvs();
  rmSync(root, { recursive: true, force: true });
});

async function fixture() {
  const config = await import('./config');
  function login(userId: string) {
    const c = config.loadConfig();
    c.auth = { userId, accessToken: `fixture-${userId}`, expiresAt: 9999999999999 };
    config.saveConfig(c);
  }
  login('tenant|A');
  return { config, login };
}
const account = (apiKey: string): ExchangeAccountConfig => ({
  name: 'Main',
  exchange: 'bybit',
  apiKey,
  apiSecret: 'fixture-secret',
});

describe('CLI cross-user isolation (synthetic data only)', () => {
  it('isolates same-name accounts, defaults, overwrite and deletion; preserves the single-user profile', async () => {
    const { config, login } = await fixture();
    config.addAccount(account('A'));
    login('tenant|B');
    expect(config.getAccount()).toBeUndefined();
    expect(config.loadConfig().defaultAccount).toBeUndefined();
    expect(config.removeAccount('Main')).toBe(false);
    config.addAccount(account('B'));
    login('tenant|A');
    expect(config.getAccount()?.apiKey).toBe('A');
    expect(config.loadConfig().defaultAccount).toBe('Main');
    login('tenant|A');
    expect(config.getAccount()?.apiKey).toBe('A');
    login('tenant|B');
    expect(config.getAccount()?.apiKey).toBe('B');
  });

  it('isolates live vars, aliases, sync queue and all safety settings across a switch', async () => {
    const { login } = await fixture();
    const { FilePreprocessStore } = await import('./preprocess-store');
    const store = new FilePreprocessStore();
    store.setVar('note', 'A');
    store.setAlias('fixture', 'A-command');
    store.setLiveVar('live', '@position');
    store.setTrackedVar('track', 'max', 'price');
    store.addToWhitelist('BTCUSDT');
    store.setFatfinger('BTCUSDT', 7);
    store.setSimulation(true);
    store.setBudget(123);
    store.addBudgetSpent(2);
    login('tenant|B');
    expect(store.getVars()).toEqual({});
    expect(store.getAliases()).toEqual({});
    expect(store.getLiveVars()).toEqual({});
    expect(store.getTrackedVars()).toEqual({});
    expect(store.getWhitelist()).toEqual([]);
    expect(store.getFatfinger('BTCUSDT')).toBeUndefined();
    expect(store.getSimulation()).toBe(false);
    expect(store.getBudget()).toBeUndefined();
    expect(store.getBudgetSpent()).toBe(0);
    expect(store.getAliasSyncState().pending).toEqual([]);
    store.unsetAllVars();
    store.unsetAllAliases();
    login('tenant|A');
    expect(store.getVar('note')).toBe('A');
    expect(store.getAlias('fixture')).toBe('A-command');
    expect(store.getBudget()).toBe(123);
  });

  it('isolates tasks, disabled cross-trade configs, heartbeat, listener identity, approvals, hooks and audit', async () => {
    const { login } = await fixture();
    const tasks = await import('./tasks-store');
    const cross = await import('./cross-trade-store');
    const heartbeat = await import('./cross-trade-heartbeat');
    const state = await import('./listener/state');
    const hooks = await import('./listener/hooks-store');
    const audit = await import('./audit-log');
    tasks.upsertTask({
      id: 'fixture',
      type: 'fixture',
      status: 'paused',
      createdAt: '2026-01-01',
      params: { owner: 'A' },
      progress: {},
    });
    const row = cross.buildSingleCopierConfig({ masterAccount: 'A', copierAccount: 'copy', enabled: false });
    cross.upsertCrossTradeConfig(row);
    heartbeat.writeHeartbeat({ startedAt: 1, configCount: 1 });
    state.addTrustedClient({ clientId: 'fixture-client', label: 'A', origin: 'https://v3.tealstreet.io' });
    const listenerId = state.getListenerId();
    const hook = hooks.createHook({ name: 'A', command: 'fixture' }).hook;
    audit.setAuditEnabled(true);
    audit.appendAuditEntry({ ts: '2026-01-01', account: 'A', command: 'fixture', ok: true });
    login('tenant|B');
    expect(tasks.loadTasks()).toEqual([]);
    expect(cross.listCrossTradeConfigs()).toEqual([]);
    expect(heartbeat.readFreshHeartbeat()).toBeNull();
    expect(state.isTrusted('fixture-client')).toBeUndefined();
    expect(state.getListenerId()).not.toBe(listenerId);
    expect(hooks.loadHooks()).toEqual([]);
    expect(audit.readAuditTail(10)).toEqual([]);
    expect(audit.isAuditEnabled()).toBe(false);
    tasks.clearAllTasks();
    cross.removeCrossTradeConfig(row.id);
    heartbeat.clearHeartbeat();
    state.removeAllTrustedClients();
    hooks.revokeAllHooks();
    audit.setAuditEnabled(false);
    login('tenant|A');
    expect(tasks.loadTasks()).toHaveLength(1);
    expect(cross.listCrossTradeConfigs()).toHaveLength(1);
    expect(heartbeat.readFreshHeartbeat()?.configCount).toBe(1);
    expect(state.getListenerId()).toBe(listenerId);
    expect(state.isTrusted('fixture-client')).toBeDefined();
    expect(hooks.loadHooks()[0]?.secret).toBe(hook.secret);
    expect(audit.isAuditEnabled()).toBe(true);
    expect(audit.readAuditTail(10)[0]?.account).toBe('A');
  });

  it('never attributes an ownerless legacy profile to the next login', async () => {
    mkdirSync(join(root, '.tealstreet'), { recursive: true });
    writeFileSync(join(root, '.tealstreet/config.json'), JSON.stringify({ accounts: [account('unclaimed')] }));
    writeFileSync(join(root, '.tealstreet/aliases.json'), JSON.stringify({ legacy: 'private-command' }));
    const { config } = await fixture();
    const { FilePreprocessStore } = await import('./preprocess-store');
    expect(config.listAccounts()).toEqual([]);
    expect(new FilePreprocessStore().getAliases()).toEqual({});
    expect(readFileSync(join(root, '.tealstreet/config.json'), 'utf8')).toContain('unclaimed');
  });

  it('loads legacy single-user data only for its recorded owner', async () => {
    mkdirSync(join(root, '.tealstreet'), { recursive: true });
    writeFileSync(
      join(root, '.tealstreet/config.json'),
      JSON.stringify({ accounts: [account('A')], auth: { userId: 'tenant|A', accessToken: 'fixture-A' } }),
    );
    writeFileSync(join(root, '.tealstreet/aliases.json'), JSON.stringify({ legacy: 'A-command' }));
    const { config, login } = await fixture();
    expect(config.getAccount()?.apiKey).toBe('A');
    const { FilePreprocessStore } = await import('./preprocess-store');
    expect(new FilePreprocessStore().getAlias('legacy')).toBe('A-command');
    login('tenant|B');
    expect(config.listAccounts()).toEqual([]);
    expect(new FilePreprocessStore().getAliases()).toEqual({});
  });
});

it.each(['recording', 'legacy-recording', 'scripts', 'log'])(
  'isolates real Bun %s state (no network)',
  async (mode) => {
    const { execFileSync } = await import('node:child_process');
    const fixturePath = join(import.meta.dirname, 'storage-bun.fixture.ts');
    // Runtime-only SQLite and file imports use the shipped Bun runtime.
    const profile = join(root, 'cli-owner-runtime');
    const output = execFileSync('bun', [fixturePath, mode], {
      env: {
        ...process.env,
        HOME: profile,
        USERPROFILE: profile,
        TEALSTREET_CONFIG_DIR: profile,
        TEALSTREET_CONFIG_FILE: join(profile, 'config.json'),
      },
      encoding: 'utf8',
      timeout: 20_000,
    });
    expect(output.trim().split('\n').at(-1)).toBe('PASS');
  },
);

it('does not upload A aliases through the same store after switching to B', async () => {
  const { login } = await fixture();
  const { FilePreprocessStore } = await import('./preprocess-store');
  const { syncAliases } = await import('@tealstreet/cli-core');
  const store = new FilePreprocessStore();
  store.setAlias('privateA', 'fixture-A-only');
  store.setAliasSyncState({ cursor: null, pending: [] });
  login('tenant|B');
  const sent: string[] = [];
  await syncAliases({
    owner: 'tenant|B',
    loadState: () => store.getAliasSyncState(),
    saveState: (state) => store.setAliasSyncState(state),
    getLocalAliases: () => store.getAliases(),
    applyRemote: (rows) => store.applyRemoteAliases(rows),
    call: async (rows) => {
      sent.push(...rows.map((row) => row.name));
      return { revision: 1, entries: [] };
    },
  });
  expect(sent).toEqual([]);
  login('tenant|A');
  expect(store.getAlias('privateA')).toBe('fixture-A-only');
});

it('logout clears both proxy caches and does not expose the last signed-in profile', async () => {
  const { config } = await fixture();
  config.addAccount(account('A'));
  const tokens = await import('@tealstreet/safe-cex/src/accessToken');
  const client = await import('@tealstreet/safe-cex/src/proxyTokenClient');
  const { logout } = await import('./auth');
  tokens.setProxyToken('fixture-A');
  logout();
  expect(await tokens.getProxyToken()).toBe('');
  expect(client.getCurrentToken()).toBe('');
  expect(config.listAccounts()).toEqual([]);
  expect(config.loadConfig().auth).toBeUndefined();
});

it('cancels old async scopes and refuses saved config objects from an earlier login', async () => {
  const { config, login } = await fixture();
  const { runInStorageSession } = await import('./storage-context');
  const saved = config.loadConfig();
  let release: () => void = () => {};
  const wait = new Promise<void>((resolve) => {
    release = resolve;
  });
  const oldWork = runInStorageSession(async () => {
    await wait;
    config.addAccount(account('stale'));
  });
  login('tenant|B');
  release();
  await expect(oldWork).rejects.toThrow('login changed');
  expect(() => config.saveConfig(saved)).toThrow('previous CLI login');
  expect(config.listAccounts()).toEqual([]);
});

it('keeps restarts for the same user, CONFIG_FILE profiles, and brands separate even with a shared override', async () => {
  const { execFileSync } = await import('node:child_process');
  const profile = join(root, 'cli-owner-profiles');
  const env = {
    ...process.env,
    HOME: profile,
    USERPROFILE: profile,
    TEALSTREET_CONFIG_DIR: profile,
    TEALSTREET_CONFIG_FILE: join(profile, 'a.json'),
  };
  const run = (mode: string, overrides: Record<string, string> = {}) =>
    execFileSync('bun', [join(import.meta.dirname, 'storage-bun.fixture.ts'), mode], {
      env: { ...env, ...overrides },
      encoding: 'utf8',
      timeout: 20_000,
    });
  run('seed');
  expect(run('probe')).toContain('"accounts":1,"tasks":1');
  expect(run('probe', { TEALSTREET_CONFIG_FILE: join(profile, 'b.json') })).toContain('"accounts":0,"tasks":0');
  expect(
    run('probe', {
      TEALSTREET_BRAND: 'disrupt',
      DISRUPT_CONFIG_FILE: env.TEALSTREET_CONFIG_FILE,
      DISRUPT_CONFIG_DIR: profile,
    }),
  ).toContain('"accounts":0,"tasks":0');
  run('seed', { FIXTURE_OWNER: 'B' });
  expect(run('probe')).toContain('"accounts":1,"tasks":1');
  run('seed', { FIXTURE_OWNER: 'A' });
  expect(run('probe')).toContain('"accounts":1,"tasks":1');
});

it.each([
  'tasks',
  'cross-trade',
  'heartbeat',
  'approval',
  'hook',
  'audit',
  'vars',
  'aliases',
  'whitelist',
  'fatfinger',
  'simulation',
  'budget',
])('independently fences the %s sweep reproducer', async (name) => {
  const { login } = await fixture();
  const tasks = await import('./tasks-store');
  const cross = await import('./cross-trade-store');
  const heartbeat = await import('./cross-trade-heartbeat');
  const state = await import('./listener/state');
  const hooks = await import('./listener/hooks-store');
  const audit = await import('./audit-log');
  const { FilePreprocessStore } = await import('./preprocess-store');
  const store = new FilePreprocessStore();
  const row = cross.buildSingleCopierConfig({ masterAccount: 'A', copierAccount: 'copy', enabled: false });
  const probes: Record<string, { seed(): void; read(): unknown; empty: unknown }> = {
    tasks: {
      seed: () =>
        tasks.upsertTask({
          id: 'fixture',
          type: 'fixture',
          status: 'paused',
          createdAt: '2026-01-01',
          params: {},
          progress: {},
        }),
      read: () => tasks.loadTasks(),
      empty: [],
    },
    'cross-trade': {
      seed: () => cross.upsertCrossTradeConfig(row),
      read: () => cross.listCrossTradeConfigs(),
      empty: [],
    },
    heartbeat: {
      seed: () => heartbeat.writeHeartbeat({ startedAt: 1, configCount: 1 }),
      read: () => heartbeat.readFreshHeartbeat(),
      empty: null,
    },
    approval: {
      seed: () => state.addTrustedClient({ clientId: 'fixture', label: 'A', origin: 'https://v3.tealstreet.io' }),
      read: () => state.isTrusted('fixture'),
      empty: undefined,
    },
    hook: {
      seed: () => {
        hooks.createHook({ name: 'fixture', command: 'fixture' });
      },
      read: () => hooks.loadHooks(),
      empty: [],
    },
    audit: {
      seed: () => {
        audit.setAuditEnabled(true);
        audit.appendAuditEntry({ ts: '2026-01-01', account: 'A', command: 'fixture', ok: true });
      },
      read: () => audit.readAuditTail(10),
      empty: [],
    },
    vars: { seed: () => store.setVar('fixture', 'A'), read: () => store.getVars(), empty: {} },
    aliases: { seed: () => store.setAlias('fixture', 'A'), read: () => store.getAliases(), empty: {} },
    whitelist: { seed: () => store.addToWhitelist('BTCUSDT'), read: () => store.getWhitelist(), empty: [] },
    fatfinger: {
      seed: () => store.setFatfinger('BTCUSDT', 7),
      read: () => store.getFatfinger('BTCUSDT'),
      empty: undefined,
    },
    simulation: { seed: () => store.setSimulation(true), read: () => store.getSimulation(), empty: false },
    budget: { seed: () => store.setBudget(123), read: () => store.getBudget(), empty: undefined },
  };
  const probe = probes[name];
  probe.seed();
  const original = probe.read();
  login('tenant|B');
  expect(probe.read()).toEqual(probe.empty);
  login('tenant|A');
  expect(probe.read()).toEqual(original);
});

it('isolates default exports and imports without changing explicit user-chosen paths', async () => {
  const { login } = await fixture();
  const { FilePreprocessStore } = await import('./preprocess-store');
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const { ExportCommand } = await import('./commands/export-import/ExportCommand');
  const { ImportCommand } = await import('./commands/export-import/ImportCommand');
  const store = new FilePreprocessStore();
  store.setVar('fixture', 'A');
  const adapter = createStandalonePlatformAdapter();
  mkdirSync(join(root, '.tealstreet'), { recursive: true });
  const ctx = {
    raw: 'export',
    args: { _: ['export'] },
    symbol: 'BTCUSDT',
    accountName: undefined,
    exchange: null,
    adapter,
    preprocessStore: store,
  };
  expect((await new ExportCommand().execute(ctx)).success).toBe(true);
  login('tenant|B');
  expect((await new ImportCommand().execute({ ...ctx, raw: 'import', args: { _: ['import'] } })).success).toBe(false);
  expect(store.getVars()).toEqual({});
  login('tenant|A');
  store.unsetAllVars();
  expect((await new ImportCommand().execute({ ...ctx, raw: 'import', args: { _: ['import'] } })).success).toBe(true);
  expect(store.getVar('fixture')).toBe('A');
});

it('leaves conflicting legacy ownership unclaimed', async () => {
  mkdirSync(join(root, '.tealstreet'), { recursive: true });
  writeFileSync(
    join(root, '.tealstreet/config.json'),
    JSON.stringify({ accounts: [account('A')], auth: { userId: 'tenant|A', accessToken: 'fixture' } }),
  );
  writeFileSync(
    join(root, '.tealstreet/aliases-sync.json'),
    JSON.stringify({ owner: 'tenant|B', cursor: 1, pending: [] }),
  );
  const { config } = await fixture();
  expect(config.listAccounts()).toEqual([]);
});

it('does not apply a delayed A alias response to B', async () => {
  const { login } = await fixture();
  const { FilePreprocessStore } = await import('./preprocess-store');
  const store = new FilePreprocessStore();
  store.setAlias('localA', 'fixture-A');
  let respond: (response: Response) => void = () => {};
  const fetcher = vi.fn(
    () =>
      new Promise<Response>((resolve) => {
        respond = resolve;
      }),
  );
  vi.stubGlobal('fetch', fetcher);
  const sync = await import('./alias-sync');
  sync.startAliasSync(store, () => {});
  const pending = sync.syncAliasesNow();
  expect(fetcher).toHaveBeenCalledOnce();
  login('tenant|B');
  respond(Response.json({ revision: 1, entries: [{ name: 'remoteA', command: 'fixture-A', revision: 1 }] }));
  expect(await pending).toBe(false);
  expect(store.getAliases()).toEqual({});
  sync.stopAliasSync();
  vi.unstubAllGlobals();
});

it('does not let a config loaded while signed out erase the next user', async () => {
  const config = await import('./config');
  const guest = config.loadConfig();
  config.saveConfig({ accounts: [], auth: { userId: 'A', accessToken: 'fixture-A' } });
  config.addAccount(account('A'));
  expect(() => config.saveConfig(guest)).toThrow('previous CLI login');
  expect(config.getAccount()?.apiKey).toBe('A');
});
