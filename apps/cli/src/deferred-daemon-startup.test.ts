import type { CLIPlatformAdapter, ExchangeAccountConfig, ExchangeLike } from '@tealstreet/cli-core';
import type { CrossTradeConfig } from '@tealstreet/safe-cex/src/crossTrade/types';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { deferred, evaluate, sessionFixture, sourceFunction } from './deferred-test.fixture';

const factory = vi.hoisted(() => ({ create: vi.fn<(account: ExchangeAccountConfig) => Promise<ExchangeLike>>() }));
vi.mock('@tealstreet/safe-cex/src/createExchangeFromAccount', () => ({ createExchangeFromAccount: factory.create }));
vi.mock('./auth', () => ({
  isLoggedIn: () => true,
  checkAndRefreshToken: async () => {},
  getProxyTokenFromConfig: () => 'synthetic-fixture',
}));
vi.mock('./logger', () => ({
  logger: { info() {}, warn() {}, error() {} },
  originalConsoleLog() {},
  originalConsoleWarn() {},
  originalConsoleError() {},
  isStdoutReservedForJson: () => false,
  writeAbovePrompt: (run: () => void) => run(),
}));
vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));

beforeEach(() => {
  vi.resetModules();
  factory.create.mockReset();
  const root = mkdtempSync(join(tmpdir(), 'cli-daemon-startup-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(() => {
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
});

const configs: CrossTradeConfig[] = [
  {
    enabled: true,
    masterAccount: 'source',
    masterSymbol: undefined,
    masterWeight: 1,
    copiers: [
      {
        account: 'follower',
        enabled: true,
        execution: 'Market',
        weight: 1,
        weightMode: 'Absolute',
        symbolMappings: {},
      },
    ],
  },
];
const cases = ['source', 'follower'].flatMap((account) =>
  ['create', 'start'].flatMap((wait) =>
    ['same', 'B', 'ABA'].flatMap((change) =>
      ['standalone', 'session'].map((scope) => ({ account, wait, change, scope })),
    ),
  ),
);

async function fixture(heldAccount: string, heldStep: string, fail = false) {
  const { storage, config, login } = await sessionFixture();
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const { ExchangeManager } = await import('./exchange-manager');
  for (const owner of ['B', 'A']) {
    login(owner);
    for (const name of ['source', 'follower'])
      config.addAccount({
        name,
        exchange: 'bybit',
        apiKey: `synthetic-${owner}`,
        apiSecret: 'synthetic',
        active: false,
      });
  }
  const entered = deferred(),
    release = deferred(),
    running = deferred();
  const signals = new Map<string, () => void>();
  const created: Array<{
    name: string;
    owner: string | null;
    epoch: number;
    credential: string | undefined;
    connected: boolean;
    dispose: ReturnType<typeof vi.fn>;
  }> = [];
  const starts: Array<{ name: string; owner: string | null; epoch: number }> = [];
  const writes: Array<{ name: string; owner: string | null; epoch: number }> = [];
  const adapter = createStandalonePlatformAdapter();
  const updateAccount = adapter.updateAccount.bind(adapter);
  vi.spyOn(adapter, 'updateAccount').mockImplementation((name, updates) => {
    writes.push({ name, owner: storage.getStorageOwner(), epoch: storage.getStorageEpoch() });
    return updateAccount(name, updates);
  });
  let manager: InstanceType<typeof ExchangeManager> | undefined;
  class ObservedManager extends ExchangeManager {
    constructor(platform: CLIPlatformAdapter) {
      super(platform);
      manager = this;
    }
  }
  factory.create.mockImplementation(async (account) => {
    const record = {
      name: account.name,
      owner: storage.getStorageOwner(),
      epoch: storage.getStorageEpoch(),
      credential: account.apiKey,
      connected: false,
      dispose: vi.fn(() => {
        record.connected = false;
      }),
    };
    created.push(record);
    const hold = async (step: string) => {
      if (account.name === heldAccount && step === heldStep) {
        entered.resolve();
        await release.promise;
        if (fail) throw new Error('synthetic account boot failure');
      }
    };
    await hold('create');
    const exchange: ExchangeLike & { on(): void } = {
      name: 'bybit',
      on() {},
      emitter: { on() {}, off() {}, emit() {} },
      dispose: record.dispose,
      start: async () => {
        starts.push({ name: account.name, owner: storage.getStorageOwner(), epoch: storage.getStorageEpoch() });
        await hold('start');
        record.connected = true;
      },
    };
    return exchange;
  });
  const identity = (text: string) => text;
  const run = evaluate<(opts: { configs: CrossTradeConfig[] }) => Promise<void>>(
    'let currentContext = null;\n' +
      ['dropChaseConfigs', 'collectAccountsToBoot', 'crossTradeCliPlaceOrder', 'runCrossTradeDaemon']
        .map((name) => sourceFunction('cross-trade-daemon.ts', name))
        .join('\n') +
      '\nrunCrossTradeDaemon;',
    {
      ...storage,
      ExchangeManager: ObservedManager,
      CrossTradeManager: class {
        start() {}
        stop() {}
      },
      createStandalonePlatformAdapter: () => adapter,
      listAccounts: config.listAccounts,
      console: { log() {} },
      logger: { warn() {} },
      chalk: { gray: identity, cyan: identity, yellow: identity, red: identity, green: identity },
      G: {},
      BRAND: {},
      process: { once: (name: string, fn: () => void) => signals.set(name, fn) },
      recordCrossTradeEvent() {},
      writeHeartbeat: () => running.resolve(),
      clearHeartbeat() {},
      HEARTBEAT_INTERVAL_MS: 5000,
      STATUS_INTERVAL_MS: 30000,
      setInterval: () => 1,
      clearInterval() {},
    },
  );
  const accounts = () => config.listAccounts().map(({ name, active, apiKey }) => ({ name, active, apiKey }));
  return {
    storage,
    login,
    run,
    entered,
    release,
    running,
    signals,
    created,
    starts,
    writes,
    accounts,
    getManager: () => {
      if (!manager) throw new Error('Daemon did not create its exchange manager');
      return manager;
    },
  };
}

it.each(cases)(
  'retains daemon boot owner during $account $wait: $change / $scope',
  async ({ account, wait, change, scope }) => {
    const h = await fixture(account, wait);
    const ownerEpoch = h.storage.getStorageEpoch();
    const start = () => h.run({ configs });
    const pending = (scope === 'session' ? h.storage.runInStorageSession(start) : start()).then(
      () => undefined,
      (error) => error,
    );
    await h.entered.promise;
    if (change !== 'same') {
      h.login('B');
      if (change === 'ABA') h.login('A');
    }
    const before = h.accounts(),
      writesBefore = [...h.writes];
    h.release.resolve();
    if (change === 'same') {
      await h.running.promise;
      expect(h.getManager().getConnectedAccountNames()).toEqual(['source', 'follower']);
      h.signals.get('SIGTERM')?.();
    }
    const result = await pending;
    try {
      if (change !== 'same') {
        expect(result).toBeInstanceOf(h.storage.StorageSessionCancelledError);
        expect(h.created.map(({ name, owner, credential, epoch }) => ({ name, owner, credential, epoch }))).toEqual(
          (account === 'source' ? ['source'] : ['source', 'follower']).map((name) => ({
            name,
            owner: 'A',
            credential: 'synthetic-A',
            epoch: ownerEpoch,
          })),
        );
        expect(h.writes).toEqual(writesBefore);
        expect(h.accounts()).toEqual(before);
        expect(h.getManager().getConnectedAccountNames()).toEqual([]);
        expect(h.created.every((record) => record.dispose.mock.calls.length > 0 && !record.connected)).toBe(true);
        expect(h.starts.every((start) => start.epoch === ownerEpoch)).toBe(true);
        if (wait === 'create') expect(h.starts.some((start) => start.name === account)).toBe(false);
      } else {
        expect(result).toBeUndefined();
        expect(h.created).toHaveLength(2);
        expect(h.starts.map((start) => start.name)).toEqual(['source', 'follower']);
        expect(h.writes.map((write) => write.name)).toEqual(['source', 'follower']);
        expect(h.accounts().every((account) => account.active)).toBe(true);
        expect(h.created.every((record) => record.dispose.mock.calls.length > 0 && !record.connected)).toBe(true);
      }
    } finally {
      await h.getManager().stopAll();
    }
  },
  30_000,
);

it.each(['create', 'start'])('continues the same-owner list after an ordinary %s failure', async (wait) => {
  const h = await fixture('source', wait, true);
  const pending = h.run({ configs });
  await h.entered.promise;
  h.release.resolve();
  await h.running.promise;
  expect(h.getManager().getConnectedAccountNames()).toEqual(['follower']);
  h.signals.get('SIGTERM')?.();
  await pending;
  expect(h.accounts().map((account) => account.active)).toEqual([false, true]);
  expect(h.writes.map((write) => write.name)).toEqual(['follower']);
});
