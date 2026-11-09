import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
import type { ExchangeLike, ExchangeAccountConfig } from '@tealstreet/cli-core';
import type { ScriptRecord } from '@tealstreet/script-api-core';

vi.mock('./logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() },
  originalConsoleLog: vi.fn(),
  originalConsoleWarn: vi.fn(),
  originalConsoleError: vi.fn(),
}));
vi.mock('../../../packages/safe-cex/src/health/ExchangeHealthMonitor', () => ({
  ExchangeHealthMonitor: class {
    dispose() {}
  },
}));
vi.setConfig({ testTimeout: 30_000 });
let root: string;
beforeEach(() => {
  vi.resetModules();
  root = mkdtempSync(join(tmpdir(), 'cli-owner-repl-session-'));
  vi.stubEnv('TEALSTREET_CONFIG_DIR', root);
  vi.stubEnv('TEALSTREET_CONFIG_FILE', join(root, 'config.json'));
});
afterEach(() => {
  vi.unstubAllEnvs();
  rmSync(root, { recursive: true, force: true });
});
const source = ts.createSourceFile(
  'repl.ts',
  readFileSync(join(import.meta.dirname, 'repl.ts'), 'utf8'),
  ts.ScriptTarget.Latest,
  true,
);
function functionText(name: string): string {
  const node = source.statements.find(
    (node): node is ts.FunctionDeclaration => ts.isFunctionDeclaration(node) && node.name?.text === name,
  );
  if (!node) throw new Error(`Missing real REPL function ${name}`);
  return node.getText(source).replace(/^export /, '');
}
function lineCallback(receiver: string): string {
  let callback = '';
  const visit = (node: ts.Node) => {
    if (
      ts.isCallExpression(node) &&
      ts.isPropertyAccessExpression(node.expression) &&
      node.expression.expression.getText(source) === receiver &&
      node.expression.name.text === 'on' &&
      node.arguments[0]?.getText(source) === "'line'"
    ) {
      const handler = node.arguments[1];
      if (handler) callback = handler.getText(source);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  if (!callback) throw new Error(`Missing real ${receiver} line handler`);
  return callback;
}
function evaluate(text: string, context: Record<string, unknown>): unknown {
  const js = ts.transpileModule(text, {
    compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.None },
  }).outputText;
  return vm.runInNewContext(js, Object.assign(context, { setTimeout, clearTimeout }));
}
async function fixture() {
  const config = await import('./config');
  const storage = await import('./storage-context');
  const login = (userId: string) => {
    const c = config.loadConfig();
    c.auth = { userId, accessToken: 'fixture' };
    config.saveConfig(c);
  };
  login('A');
  return { config, storage, login };
}

describe('actual readline callbacks', () => {
  it.each(['rl', 'newRl'].flatMap((receiver) => ['probe', 'exit', 'quit'].map((command) => ({ receiver, command }))))(
    '$receiver cancels queued $command before B can execute it',
    async ({ receiver, command }) => {
      const { storage, login } = await fixture();
      let release = () => {};
      const blocked = new Promise<void>((resolve) => {
        release = resolve;
      });
      const messages: string[] = [];
      const execute = vi.fn(async () => {}),
        teardown = vi.fn();
      const scope = {
        ...storage,
        commandQueue: blocked,
        input: command,
        processCommandInSession: execute,
        appendToHistory: vi.fn(),
        adapter: {
          printInfo: (message: string) => {
            messages.push(message);
            if (message === 'Goodbye!') teardown();
          },
          printError: (message: string) => messages.push(message),
        },
        console: { error: (error: unknown) => messages.push(String(error)) },
        rl: { prompt() {}, setPrompt() {} },
        newRl: { prompt() {}, setPrompt() {} },
        isMonitorMode: false,
        getPrompt: () => '',
        connectionMonitor: null,
        networkMonitor: null,
        stopListener: teardown,
        stopWebhookRelay: teardown,
        stopAliasSync: teardown,
        exchangeManager: { stopAll: teardown },
        process: { exit: teardown },
      };
      evaluate(`${functionText('processCommand')}\n(${lineCallback(receiver)})(input);`, scope);
      login('B');
      release();
      await scope.commandQueue;
      expect(execute).not.toHaveBeenCalled();
      expect(teardown).not.toHaveBeenCalled();
      expect(messages.join(' ')).toMatch(/cancel|login changed/i);
    },
  );
  it.each(['rl', 'newRl'])('%s executes unchanged-owner work normally', async (receiver) => {
    const { storage } = await fixture();
    const execute = vi.fn(async () => {});
    const scope = {
      ...storage,
      commandQueue: Promise.resolve(),
      input: 'probe',
      processCommandInSession: execute,
      appendToHistory: vi.fn(),
      adapter: { printError: vi.fn() },
      console: { error: vi.fn() },
      rl: { prompt() {}, setPrompt() {} },
      newRl: { prompt() {}, setPrompt() {} },
      isMonitorMode: false,
      getPrompt: () => '',
    };
    evaluate(`${functionText('processCommand')}\n(${lineCallback(receiver)})(input);`, scope);
    await scope.commandQueue;
    expect(execute).toHaveBeenCalledWith('probe', {});
  });
});

it.each([false, true])(
  'binds an actual paused footer invocation to its starting user (switch=%s)',
  async (switchUser) => {
    const { storage, login } = await fixture();
    const { ExchangeManager } = await import('./exchange-manager');
    const { createStandalonePlatformAdapter } = await import('./platform-adapter');
    const { createStandaloneCoreAdapter } = await import('./script-adapter');
    const { runScriptFromHostWithAdapter } = await import('./script-runner');
    const { CORE_API_VERSION } = await import('@tealstreet/script-api-core');
    const adapter = createStandalonePlatformAdapter(),
      manager = new ExchangeManager(adapter);
    const account: ExchangeAccountConfig = { name: 'Main', exchange: 'bybit', apiKey: 'fixture', apiSecret: 'fixture' };
    const exchange = (total: number): ExchangeLike => ({
      name: 'bybit',
      emitter: { on() {}, off() {}, emit() {} },
      store: { balance: { total, free: total, used: 0, upnl: 0 } },
    });
    manager.register('Main', exchange(100), account);
    let release = () => {},
      enter = () => {};
    const paused = new Promise<void>((resolve) => {
      release = resolve;
    });
    const started = new Promise<void>((resolve) => {
      enter = resolve;
    });
    const seen: number[] = [];
    const record: ScriptRecord = {
      id: 'fixture',
      name: 'fixture',
      targets: ['core'],
      coreApiVersion: CORE_API_VERSION,
      capabilities: { accounts: [], writes: [], idempotent: true, storage: { enabled: false, quotaBytes: 0 } },
      fn: async (_args, api) => {
        enter();
        await paused;
        const balance = await api.getBalance({ account: 'Main' });
        if (balance) seen.push(balance.total);
      },
    };
    const session = { currentAccountName: 'Main', currentSymbol: 'BTCUSDT', currentExchange: null };
    const scope = {
      ...storage,
      footerScriptInflight: new Set<string>(),
      footerScriptCache: new Map<string, string>(),
      lookupScript: () => record,
      FOOTER_SCRIPT_TIMEOUT_MS: 2000,
      createStandaloneCoreAdapter,
      exchangeManager: manager,
      session,
      adapter,
      runScriptFromHostWithAdapter,
      getCliTranspile: () => async (text: string) => text,
      FOOTER_READONLY_CAPS: { accounts: [], writes: [], idempotent: true, storage: { enabled: false, quotaBytes: 0 } },
    };
    const pending = evaluate(`${functionText('runFooterScript')}\nrunFooterScript('fixture');`, scope);
    await Promise.race([
      started,
      Promise.resolve(pending).then(() => {
        throw new Error('Footer completed before entering the script');
      }),
    ]);
    if (switchUser) {
      login('B');
      manager.register('Main', exchange(200), account);
    }
    release();
    await pending;
    expect(seen).toEqual(switchUser ? [] : [100]);
    await manager.stopAll();
  },
);

it('lets B proceed without waiting for A’s cancelled queue and never runs A afterwards', async () => {
  const { storage, login } = await fixture();
  let release = () => {};
  const blocked = new Promise<void>((resolve) => {
    release = resolve;
  });
  const executeA = vi.fn(),
    executeB = vi.fn();
  const notice = vi.fn();
  const cancelled = storage.enqueueSessionWork(blocked, executeA, notice).catch((error: unknown) => {
    expect(error).toBeInstanceOf(storage.QueuedSessionCancelledError);
  });
  login('B');
  await storage.enqueueSessionWork(cancelled, executeB, notice);
  expect(executeB).toHaveBeenCalledOnce();
  expect(executeA).not.toHaveBeenCalled();
  expect(notice).toHaveBeenCalledOnce();
  release();
  await blocked;
  await Promise.resolve();
  expect(executeA).not.toHaveBeenCalled();
});

it('rejects retained host bindings outside the old async scope after login changes', async () => {
  const { login } = await fixture();
  const { ExchangeManager } = await import('./exchange-manager');
  const { createStandalonePlatformAdapter } = await import('./platform-adapter');
  const { createStandaloneHostBindings } = await import('./script-adapter');
  const manager = new ExchangeManager(createStandalonePlatformAdapter());
  const bindings = createStandaloneHostBindings(manager, {
    currentAccountName: 'Main',
    currentSymbol: 'BTCUSDT',
    currentExchange: null,
  });
  expect(bindings.readActiveAccount()).toBe('Main');
  login('B');
  expect(() => bindings.getStore('Main')).toThrow('login changed');
  expect(() => bindings.readActiveAccount()).toThrow('login changed');
  expect(() => bindings.readActiveSymbol()).toThrow('login changed');
});
