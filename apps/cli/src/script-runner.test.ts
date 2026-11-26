/**
 * Phase 2 Step 5 acceptance — host shim's idempotent retry gate +
 * capture-writeback per SPEC §4.3 / §7.4 / §7.5.
 *
 * Uses the lower-level `runScriptFromHostWithAdapter` so the test can
 * inject a minimal stub adapter without spinning up an `ExchangeManager`
 * or `CLIPlatformAdapter`. The full `runScriptFromHost` is a thin
 * wrapper around it.
 */
import { describe, expect, it, vi } from 'vitest';

import type { Capabilities } from '@tealstreet/script-api-core';
import type { CoreAdapter, ScriptRecord } from '@tealstreet/script-api-core';
import { createCoreAdapter } from '@tealstreet/script-api-core';
import { OrderSide, OrderType } from '@tealstreet/safe-cex/src/types';
import type { PreprocessStore } from '@tealstreet/cli-core';

import { runScriptFromHostWithAdapter } from './script-runner';

// ───────────────────────────────────────────────────────────────────────────
// Minimal CoreAdapter stub. Only the fields runScript actually reads.
// ───────────────────────────────────────────────────────────────────────────

const buildStubAdapter = (): CoreAdapter => {
  const printCalls: { text: string; kind: string }[] = [];
  const adapter = {
    // This test double has one immutable account owner.
    captureAccountOwners: () => () => undefined,
    getActiveAccount: () => 'mybybit',
    getActiveAccounts: () => ['mybybit'],
    getActiveSymbol: () => 'BTCUSDT',
    getActiveExchange: () => 'bybit',
    getAccounts: () => [{ name: 'mybybit', exchange: 'bybit', isStreaming: true }],
    getMarkets: () => [],
    getMarket: () => undefined,
    getTicker: () => undefined,
    getTickers: () => [],
    getPositions: () => [],
    getHistoricalPosition: () => undefined,
    getOrders: () => [],
    getBalance: () => undefined,
    getCombinedBalance: () => ({
      totalEquity: 0,
      totalAvailable: 0,
      totalUsed: 0,
      totalUpnl: 0,
      longExposure: 0,
      shortExposure: 0,
      currency: 'USD',
    }),
    getOrderbook: () => undefined,
    getCandles: () => [],
    placeOrder: async () => ({ orderIds: [] }),
    placeOrders: async () => ({ orderIds: [] }),
    cancelOrder: async () => {},
    cancelOrders: async () => ({ canceled: 0 }),
    updateOrder: async () => {},
    closePosition: async () => ({ orderIds: [] }),
    setLeverage: async () => {},
    setPositionMode: async () => {},
    setMarginType: async () => {},
    runCommand: async () => ({ ok: true, capturedIds: [], output: [] }),
    print: (text: string, kind: string) => {
      printCalls.push({ text, kind });
    },
    signer: undefined,
  } satisfies CoreAdapter & { signer: unknown };
  return adapter as CoreAdapter;
};

// Identity transpile — Phase 2 tests feed plain JS bodies.
const identityTranspile = async (src: string) => src;

// ───────────────────────────────────────────────────────────────────────────
// Minimal PreprocessStore stub. Only `setVar` matters here.
// ───────────────────────────────────────────────────────────────────────────

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
    setVars: (entries: Record<string, string>) => {
      vars.clear();
      for (const [k, v] of Object.entries(entries)) vars.set(k, v);
    },
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

const baseCaps = (overrides: Partial<Capabilities> = {}): Capabilities => ({
  accounts: ['mybybit'],
  writes: [],
  idempotent: false,
  storage: { enabled: true, quotaBytes: 102_400 },
  ...overrides,
});

const buildRecord = (overrides: Partial<ScriptRecord> = {}): ScriptRecord => ({
  id: 'uuid-1',
  name: 'test-script',
  code: '',
  targets: ['core'],
  coreApiVersion: '1.0.0',
  capabilities: baseCaps(),
  ...overrides,
});

it.each([
  { replace: false, late: false },
  { replace: true, late: false },
  { replace: false, late: true },
  { replace: true, late: true },
])('does not capture a placement completed by a retired owner (%j)', async ({ replace, late }) => {
  let entered!: () => void;
  const started = new Promise<void>((done) => {
    entered = done;
  });
  let reply!: (ids: string[]) => void;
  const response = new Promise<string[]>((done) => {
    reply = done;
  });
  const aWrite = vi.fn(() => {
    entered();
    return response;
  });
  const bWrite = vi.fn(async () => ['b-id']);
  let current = { name: 'binance', placeOrder: aWrite };
  const writes = vi.fn();
  const adapter = createCoreAdapter({
    getExchange: () => current as never,
    getStore: () =>
      ({
        markets: [
          {
            symbol: 'BTCUSDT',
            precision: { amount: 0.1, price: 0.5 },
            limits: { amount: { min: 0, max: 0 } },
            isSpot: false,
          },
        ],
        orders: [],
        positions: [],
      }) as never,
    readActiveAccount: () => 'main',
    readActiveSymbol: () => 'BTCUSDT',
    readStreamingAccounts: () => ['main'],
    readAllAccounts: () => [{ name: 'main', exchange: 'binance' }],
    readOrderbook: () => undefined,
    onWrite: writes,
  });
  const store = buildStubStore();
  const pending = runScriptFromHostWithAdapter(
    { adapter, transpile: identityTranspile },
    {
      record: buildRecord({
        capabilities: baseCaps({ accounts: ['main'], writes: ['placeOrder'] }),
        fn: async (_args, api) => {
          const result = await api.exchange.placeOrder({
            symbol: 'BTCUSDT',
            side: OrderSide.Buy,
            type: OrderType.Market,
            amount: 1,
          });
          api.io.return(result.orderIds[0]);
          if (late)
            return {
              then: (done: (value: unknown) => void) => {
                done(undefined);
                // Replace after core's final check but before the CLI's await resumes.
                if (replace)
                  queueMicrotask(() => {
                    current = { name: 'binance', placeOrder: bWrite };
                  });
              },
            };
        },
      }),
      rawArgs: '',
      invokedFrom: 'cli',
      capture: { captureTo: 'owner_result', preprocessStore: store },
    },
  );
  await started;
  if (replace && !late) current = { name: 'binance', placeOrder: bWrite };
  reply(['a-completed']);
  const result = await pending;
  expect(aWrite).toHaveBeenCalledTimes(1);
  expect(bWrite).not.toHaveBeenCalled();
  if (replace) {
    expect(store.vars.has('owner_result')).toBe(false);
    expect(store.setVar).not.toHaveBeenCalled();
    if (!late) expect(writes).not.toHaveBeenCalled();
    expect(result).toMatchObject({ ok: false, error: expect.stringMatching(/owner changed/) });
  } else {
    expect(store.vars.get('owner_result')).toBe('a-completed');
    expect(result.ok).toBe(true);
  }
});

// ───────────────────────────────────────────────────────────────────────────
// Tests
// ───────────────────────────────────────────────────────────────────────────

describe('runScriptFromHost — idempotent retry gate (SPEC §4.3 / §7.5)', () => {
  it('refuses retry of a non-idempotent script with the exact spec error', async () => {
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ name: 'myScript', code: 'api.io.return("ran");' }),
        rawArgs: '',
        invokedFrom: 'cli',
        retry: { isRetry: true, scriptName: 'myScript' },
      },
    );
    expect(result.ok).toBe(false);
    // SPEC §4.3 exact-string contract.
    expect(result.error).toBe("script 'myScript' is not marked idempotent; cannot retry");
  });

  it('allows retry when capabilities.idempotent === true', async () => {
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({
          name: 'myScript',
          code: 'api.io.return("ran");',
          capabilities: baseCaps({ idempotent: true }),
        }),
        rawArgs: '',
        invokedFrom: 'cli',
        retry: { isRetry: true, scriptName: 'myScript' },
      },
    );
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('ran');
  });

  it('skips the gate when not retrying (default path)', async () => {
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.return("ok");' }),
        rawArgs: '',
        invokedFrom: 'cli',
        // No retry → idempotent flag is irrelevant; script runs.
      },
    );
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('ok');
  });
});

describe('runScriptFromHost — capture-writeback (SPEC §7.4)', () => {
  it('writes string returnValue to the captureTo var, no sibling .json', async () => {
    const store = buildStubStore();
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.return("order-id-123");' }),
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'x', preprocessStore: store },
      },
    );
    expect(result.ok).toBe(true);
    expect(store.vars.get('x')).toBe('order-id-123');
    expect(store.vars.has('x.json')).toBe(false);
  });

  it('comma-joins string[] returnValue (the `cancel @x` interop case)', async () => {
    const store = buildStubStore();
    await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.return(["o1", "o2", "o3"]);' }),
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'ids', preprocessStore: store },
      },
    );
    expect(store.vars.get('ids')).toBe('o1,o2,o3');
    expect(store.vars.has('ids.json')).toBe(false);
  });

  it('JSON-stringifies object returnValue AND sets sibling .json', async () => {
    const store = buildStubStore();
    await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.return({ orderId: "abc", filled: 1 });' }),
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'out', preprocessStore: store },
      },
    );
    expect(store.vars.get('out')).toBe('{"orderId":"abc","filled":1}');
    expect(store.vars.get('out.json')).toBe('{"orderId":"abc","filled":1}');
  });

  it('writes scalar number / boolean stringified, no sibling .json', async () => {
    const store = buildStubStore();
    await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.return(42);' }),
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'n', preprocessStore: store },
      },
    );
    expect(store.vars.get('n')).toBe('42');
    expect(store.vars.has('n.json')).toBe(false);
  });

  it('does NOT write when script returns nothing', async () => {
    const store = buildStubStore();
    await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.print("hi");' }),
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'x', preprocessStore: store },
      },
    );
    expect(store.vars.size).toBe(0);
  });

  it('does NOT write when the script failed (ok:false)', async () => {
    const store = buildStubStore();
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({
          code: 'api.io.return("almost"); throw new Error("boom");',
        }),
        rawArgs: '',
        invokedFrom: 'cli',
        capture: { captureTo: 'x', preprocessStore: store },
      },
    );
    expect(result.ok).toBe(false);
    expect(store.vars.size).toBe(0);
  });

  it('does NOT write when capture is omitted (default path)', async () => {
    const store = buildStubStore();
    const result = await runScriptFromHostWithAdapter(
      { adapter: buildStubAdapter(), transpile: identityTranspile },
      {
        record: buildRecord({ code: 'api.io.return("returned-but-not-captured");' }),
        rawArgs: '',
        invokedFrom: 'cli',
        // No capture context.
      },
    );
    expect(result.returnValue).toBe('returned-but-not-captured');
    expect(store.vars.size).toBe(0);
  });
});
