/**
 * End-to-end tests covering the FULL chain a `script:<name>` invocation
 * walks through in production:
 *
 *   .ts file on disk
 *     → `scripts-store.loadScripts()` (real fs read + dynamic `import()`)
 *     → `LoadedScript.fn` ref populated
 *     → `script-runner.runScriptFromHostWithAdapter()` (real host shim,
 *       no `ExchangeManager` / `CLIPlatformAdapter` wiring)
 *     → `script-api-core.runScript()` (validateTargets, parseScriptArgs,
 *       buildApi, real `InProcessProxy` over a mock `CoreAdapter`,
 *       capability gates, sink-based io capture)
 *     → assertions against the mock adapter + sink.
 *
 * Differences from the existing unit tests:
 *   - `scripts-store.test.ts` exercises the file → cache leg but stops
 *     at `LoadedScript`. It never invokes `fn`.
 *   - `script-runner.test.ts` exercises the runner leg but constructs
 *     `ScriptRecord` inline with `code:` strings; it tests the `code`
 *     (transpile + IIFE) path, NEVER the `fn` (file-loaded) path. The
 *     `fn` path is what the CLI binary actually uses in production.
 *
 * These tests bridge the two legs. Each test drops a real `.ts` file,
 * loads it, then runs it through the host shim to verify the SAME chain
 * a user's `script:foo` hits.
 */
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Capabilities, CoreAdapter } from '@tealstreet/script-api-core';
import type { PreprocessStore } from '@tealstreet/cli-core';

// Redirect HOME before importing scripts-store (module-scope constants
// resolve from `os.homedir()` once). The original env values are captured
// so `afterAll` can restore them and remove the temp root — without that
// cleanup, every test run leaves a `tealstreet-script-e2e-*` directory
// under `os.tmpdir()` plus a permanently-redirected HOME inside this
// process (harmless for vitest's single-process model, but the right
// hygiene + matches what Copilot flagged on PR #5700).
const ORIGINAL_HOME = process.env.HOME;
const ORIGINAL_USERPROFILE = process.env.USERPROFILE;
const TMP_BASE = fs.mkdtempSync(path.join(os.tmpdir(), 'tealstreet-script-e2e-'));
process.env.HOME = TMP_BASE;
process.env.USERPROFILE = TMP_BASE;

const { clearScriptsCache, loadScripts, lookupScript, SCRIPTS_DIR } = await import('./scripts-store');
const { runScriptFromHostWithAdapter } = await import('./script-runner');

// ───────────────────────────────────────────────────────────────────────────
// Test scaffolding — stub adapter + stub store reused from
// `script-runner.test.ts` shape, kept terse here.
// ───────────────────────────────────────────────────────────────────────────

interface PrintCall {
  readonly text: string;
  readonly kind: string;
}

/**
 * Build a `CoreAdapter` stub with side-channel arrays the tests assert
 * against. `satisfies CoreAdapter & { ... }` gives compile-time
 * checking on the full adapter shape — if `CoreAdapter` gains a method,
 * this fixture stops compiling instead of silently drifting and only
 * failing at runtime when a script happens to hit the missing path
 * (per Copilot review on PR #5700). Matches the typed-stub pattern in
 * `script-runner.test.ts`.
 */
const buildStubAdapter = (
  overrides: Partial<{
    activeAccount: string;
    accounts: ReturnType<CoreAdapter['getAccounts']>;
  }> = {},
) => {
  const printCalls: PrintCall[] = [];
  const placeOrderCalls: unknown[] = [];
  const runCommandCalls: { command: string; opts: unknown }[] = [];
  return {
    getActiveAccount: () => overrides.activeAccount ?? 'mybybit',
    getActiveAccounts: () => [overrides.activeAccount ?? 'mybybit'],
    getActiveSymbol: () => 'BTCUSDT',
    getActiveExchange: () => 'bybit',
    getAccounts: () => overrides.accounts ?? [{ name: 'mybybit', exchange: 'bybit', isStreaming: true }],
    getMarkets: () => [],
    getMarket: () => undefined,
    // No e2e test calls `getTicker` directly. Return `undefined` instead
    // of a partial Ticker so `satisfies CoreAdapter` doesn't trip on the
    // ~7 required Ticker fields a real adapter would surface — the unit
    // test stub in `script-runner.test.ts` does the same thing.
    getTicker: () => undefined,
    getTickers: () => [],
    getPositions: () => [],
    getHistoricalPosition: () => undefined,
    getOrders: () => [],
    getBalance: () => ({ total: 1000, free: 800, used: 200, upnl: 0 }),
    getCombinedBalance: () => ({
      totalEquity: 1000,
      totalAvailable: 800,
      totalUsed: 200,
      totalUpnl: 0,
      longExposure: 0,
      shortExposure: 0,
      currency: 'USD',
    }),
    getOrderbook: () => undefined,
    getCandles: () => [],
    placeOrder: async (opts: unknown) => {
      placeOrderCalls.push(opts);
      return { orderIds: ['ord-1'] };
    },
    placeOrders: async () => ({ orderIds: [] }),
    cancelOrder: async () => {},
    cancelOrders: async () => ({ canceled: 0 }),
    updateOrder: async () => {},
    closePosition: async () => ({ orderIds: [] }),
    setLeverage: async () => {},
    setPositionMode: async () => {},
    setMarginType: async () => {},
    runCommand: async (command: string, opts: unknown) => {
      runCommandCalls.push({ command, opts });
      return { ok: true, capturedIds: [], output: [] };
    },
    print: (text: string, kind: string) => {
      printCalls.push({ text, kind });
    },
    // Test-only side-channels for assertions.
    printCalls,
    placeOrderCalls,
    runCommandCalls,
    signer: undefined,
  } satisfies CoreAdapter & {
    readonly printCalls: PrintCall[];
    readonly placeOrderCalls: unknown[];
    readonly runCommandCalls: { command: string; opts: unknown }[];
    readonly signer: unknown;
  };
};

const buildStubStore = (): PreprocessStore & { vars: Map<string, string> } => {
  const vars = new Map<string, string>();
  return {
    vars,
    getVar: (n: string) => vars.get(n),
    setVar: vi.fn((n: string, v: string) => {
      vars.set(n, v);
    }),
    unsetVar: (n: string) => vars.delete(n),
    getVars: () => Object.fromEntries(vars),
    listVars: () => Array.from(vars.keys()),
    setVars: () => {},
    getAlias: () => undefined,
    setAlias: () => {},
    unsetAlias: () => false,
    getAliases: () => ({}),
    listAliases: () => [],
    setAliases: () => {},
    getSimulation: () => false,
    setSimulation: () => {},
    setTrackedVar: () => {},
    getTrackedVar: () => undefined,
    setLiveVar: () => {},
    getLiveVar: () => undefined,
    isLiveVar: () => false,
    listLiveVars: () => [],
    isTrackedVar: () => false,
    listTrackedVars: () => [],
    getTrackedVarTracker: () => undefined,
    getLiveVarSource: () => undefined,
    isPaused: () => false,
    setPaused: () => {},
    getRecording: () => null,
    setRecording: () => {},
  } as unknown as PreprocessStore & { vars: Map<string, string> };
};

const writeScript = (name: string, body: string): void => {
  if (!fs.existsSync(SCRIPTS_DIR)) fs.mkdirSync(SCRIPTS_DIR, { recursive: true });
  fs.writeFileSync(path.join(SCRIPTS_DIR, `${name}.ts`), body);
};

const cleanScriptsDir = (): void => {
  if (fs.existsSync(SCRIPTS_DIR)) fs.rmSync(SCRIPTS_DIR, { recursive: true, force: true });
  clearScriptsCache();
};

// Transpile is unused by the `fn` path but `runContext.transpile` is
// required on the type. `runScript` checks `record.fn` first and only
// reaches transpile when `code` is the source. Identity satisfies the
// shape without doing any work for file-loaded scripts.
const identityTranspile = async (src: string) => src;

const META_BLOCK = (caps: Partial<Capabilities> = {}): string => {
  const merged = {
    accounts: ['mybybit'],
    writes: [] as string[],
    idempotent: true,
    storage: { enabled: false, quotaBytes: 0 },
    ...caps,
  };
  return `
import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';
export const meta: ScriptMeta = {
  targets: ['core', 'cli'],
  capabilities: ${JSON.stringify(merged)},
};
`;
};

beforeAll(() => {
  // Sanity: the redirected HOME is what scripts-store actually computed
  // SCRIPTS_DIR from. If the imports above raced the env mutation, this
  // catches it loudly instead of leaking writes to the real ~/.tealstreet.
  expect(SCRIPTS_DIR.startsWith(TMP_BASE)).toBe(true);
});

beforeEach(() => {
  cleanScriptsDir();
});

afterEach(() => {
  cleanScriptsDir();
});

afterAll(() => {
  // Remove the entire temp root (the per-suite mkdtemp result) so
  // repeated local/CI runs don't accumulate `tealstreet-script-e2e-*`
  // dirs under `os.tmpdir()`. Restore the captured HOME/USERPROFILE so
  // any sibling test that imports `os.homedir()` after us still sees
  // the user's real env.
  if (fs.existsSync(TMP_BASE)) {
    fs.rmSync(TMP_BASE, { recursive: true, force: true });
  }
  if (ORIGINAL_HOME === undefined) delete process.env.HOME;
  else process.env.HOME = ORIGINAL_HOME;
  if (ORIGINAL_USERPROFILE === undefined) delete process.env.USERPROFILE;
  else process.env.USERPROFILE = ORIGINAL_USERPROFILE;
});

// ───────────────────────────────────────────────────────────────────────────
// 1. file → fn → output (happy path)
// ───────────────────────────────────────────────────────────────────────────

describe('file → loader → runner — happy path', () => {
  it('runs a file-loaded script via the fn path and captures io output', async () => {
    writeScript(
      'ping',
      `${META_BLOCK()}
export default async function (args: ScriptArgs, api: CoreApi): Promise<void> {
  const acct = await api.getActiveAccount();
  api.io.print(\`pong from \${acct}\`, 'success');
  api.io.return('pong');
}
`,
    );

    await loadScripts();
    const record = lookupScript('ping');
    expect(record).toBeDefined();
    expect(typeof record!.fn).toBe('function');
    // No `code` on file-loaded records — the runner must take the fn path.
    expect(record!.code).toBeUndefined();

    const adapter = buildStubAdapter();
    const result = await runScriptFromHostWithAdapter(
      { adapter, transpile: identityTranspile },
      { record: record!, rawArgs: '', invokedFrom: 'cli' },
    );

    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('pong');
    expect(result.output).toEqual([{ text: 'pong from mybybit', kind: 'success' }]);
  });

  it('passes parsed args through to the script — positional + flags + raw', async () => {
    writeScript(
      'echo',
      `${META_BLOCK()}
export default async function (args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return({
    positional: args.positional,
    flag_n: args.flags.n,
    raw: args.raw,
  });
}
`,
    );

    await loadScripts();
    const record = lookupScript('echo')!;
    const adapter = buildStubAdapter();
    const result = await runScriptFromHostWithAdapter(
      { adapter, transpile: identityTranspile },
      { record, rawArgs: 'first second --n=3', invokedFrom: 'cli' },
    );

    expect(result.ok).toBe(true);
    expect(result.returnValue).toEqual({
      positional: ['first', 'second'],
      flag_n: '3',
      raw: 'first second --n=3',
    });
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 2. invokedFrom propagates through api.meta (SPEC §6 contract)
// ───────────────────────────────────────────────────────────────────────────

describe('invokedFrom — propagates from host into api.meta', () => {
  const SCRIPT = `${META_BLOCK()}
export default async function (args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return(api.meta.invokedFrom);
}
`;

  it.each<'cli' | 'webhook' | 'hotkey' | 'inline'>(['cli', 'webhook', 'hotkey', 'inline'])(
    'surfaces invokedFrom=%s to the running script',
    async (invokedFrom) => {
      writeScript('whoinvoked', SCRIPT);
      await loadScripts();
      const record = lookupScript('whoinvoked')!;
      const adapter = buildStubAdapter();
      const result = await runScriptFromHostWithAdapter(
        { adapter, transpile: identityTranspile },
        { record, rawArgs: '', invokedFrom },
      );
      expect(result.ok).toBe(true);
      expect(result.returnValue).toBe(invokedFrom);
    },
  );
});

// ───────────────────────────────────────────────────────────────────────────
// 3. Capability gates — runtime catches violations
// ───────────────────────────────────────────────────────────────────────────

describe('capability gates — runtime enforces declared writes', () => {
  it('blocks placeOrder when writes does NOT include placeOrder', async () => {
    writeScript(
      'badPlace',
      `${META_BLOCK({ writes: [] })}
export default async function (args: ScriptArgs, api: CoreApi): Promise<void> {
  await api.exchange.placeOrder({
    symbol: 'BTCUSDT',
    side: 'buy',
    type: 'market',
    amount: 1,
  });
  api.io.return('should-not-reach');
}
`,
    );
    await loadScripts();
    const record = lookupScript('badPlace')!;
    const adapter = buildStubAdapter();
    const result = await runScriptFromHostWithAdapter(
      { adapter, transpile: identityTranspile },
      { record, rawArgs: '', invokedFrom: 'cli' },
    );

    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/placeOrder/);
    // Critical: the adapter must NEVER have been called when the gate
    // fires. The gate sits BEFORE the proxy hands off to the adapter.
    expect(adapter.placeOrderCalls).toHaveLength(0);
  });

  it('allows placeOrder when writes includes it — value reaches the adapter', async () => {
    writeScript(
      'goodPlace',
      `${META_BLOCK({ writes: ['placeOrder'] })}
export default async function (args: ScriptArgs, api: CoreApi): Promise<void> {
  const r = await api.exchange.placeOrder({
    symbol: 'BTCUSDT',
    side: 'buy',
    type: 'market',
    amount: 0.001,
  });
  api.io.return(r.orderIds[0]);
}
`,
    );
    await loadScripts();
    const record = lookupScript('goodPlace')!;
    const adapter = buildStubAdapter();
    const result = await runScriptFromHostWithAdapter(
      { adapter, transpile: identityTranspile },
      { record, rawArgs: '', invokedFrom: 'cli' },
    );
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('ord-1');
    expect(adapter.placeOrderCalls).toHaveLength(1);
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 4. Idempotent retry gate — fires through the file path too
// ───────────────────────────────────────────────────────────────────────────

describe('idempotent retry gate — applies regardless of fn-vs-code path', () => {
  it('refuses retry of a non-idempotent file-loaded script', async () => {
    writeScript(
      'fragile',
      `${META_BLOCK({ idempotent: false })}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('done');
}
`,
    );
    await loadScripts();
    const record = lookupScript('fragile')!;
    const adapter = buildStubAdapter();
    const result = await runScriptFromHostWithAdapter(
      { adapter, transpile: identityTranspile },
      {
        record,
        rawArgs: '',
        invokedFrom: 'cli',
        retry: { isRetry: true, scriptName: 'fragile' },
      },
    );
    expect(result.ok).toBe(false);
    expect(result.error).toBe("script 'fragile' is not marked idempotent; cannot retry");
  });

  it('allows retry of an idempotent file-loaded script', async () => {
    writeScript(
      'safe',
      `${META_BLOCK({ idempotent: true })}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('done');
}
`,
    );
    await loadScripts();
    const record = lookupScript('safe')!;
    const adapter = buildStubAdapter();
    const result = await runScriptFromHostWithAdapter(
      { adapter, transpile: identityTranspile },
      {
        record,
        rawArgs: '',
        invokedFrom: 'cli',
        retry: { isRetry: true, scriptName: 'safe' },
      },
    );
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('done');
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 5. Capture writeback — full chain
// ───────────────────────────────────────────────────────────────────────────

describe('capture writeback — file path + capture context together', () => {
  it('writes a string return to the captureTo var via the host shim', async () => {
    writeScript(
      'producer',
      `${META_BLOCK()}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('order-77');
}
`,
    );
    await loadScripts();
    const record = lookupScript('producer')!;
    const store = buildStubStore();
    await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record,
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'lastId', preprocessStore: store },
      },
    );
    expect(store.vars.get('lastId')).toBe('order-77');
    expect(store.vars.has('lastId.json')).toBe(false);
  });

  it('object return → sibling .json gets set', async () => {
    writeScript(
      'producerObj',
      `${META_BLOCK()}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return({ orderId: 'abc', side: 'buy' });
}
`,
    );
    await loadScripts();
    const record = lookupScript('producerObj')!;
    const store = buildStubStore();
    await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record,
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'last', preprocessStore: store },
      },
    );
    expect(store.vars.get('last')).toBe('{"orderId":"abc","side":"buy"}');
    expect(store.vars.get('last.json')).toBe('{"orderId":"abc","side":"buy"}');
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 6. Reload picks up file edits (the cache-buster contract)
// ───────────────────────────────────────────────────────────────────────────

describe('script reload picks up file edits via the dynamic-import cache-buster', () => {
  it('re-runs the script body after an in-place edit + reload', async () => {
    writeScript(
      'editable',
      `${META_BLOCK()}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('v1');
}
`,
    );
    await loadScripts();
    {
      const record = lookupScript('editable')!;
      const result = await runScriptFromHostWithAdapter(
        { adapter: buildStubAdapter(), transpile: identityTranspile },
        { record, rawArgs: '', invokedFrom: 'cli' },
      );
      expect(result.returnValue).toBe('v1');
    }

    // In-place edit.
    writeScript(
      'editable',
      `${META_BLOCK()}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('v2');
}
`,
    );
    await loadScripts(); // simulates REPL `script reload`

    const record = lookupScript('editable')!;
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      { record, rawArgs: '', invokedFrom: 'cli' },
    );
    expect(result.returnValue).toBe('v2');
  });
});

// ───────────────────────────────────────────────────────────────────────────
// 7. Multiple scripts coexist; lookup resolves the right one
// ───────────────────────────────────────────────────────────────────────────

describe('registry — multiple scripts coexist; lookup picks the right fn', () => {
  it('runs the right script when several are loaded', async () => {
    writeScript(
      'alpha',
      `${META_BLOCK()}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('a');
}
`,
    );
    writeScript(
      'beta',
      `${META_BLOCK()}
export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return('b');
}
`,
    );
    await loadScripts();

    const ra = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      { record: lookupScript('alpha')!, rawArgs: '', invokedFrom: 'cli' },
    );
    const rb = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      { record: lookupScript('beta')!, rawArgs: '', invokedFrom: 'cli' },
    );
    expect(ra.returnValue).toBe('a');
    expect(rb.returnValue).toBe('b');
  });
});
