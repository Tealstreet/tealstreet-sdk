import { describe, expect, it } from 'vitest';

import { createMockAdapter } from '../__tests__/mockAdapter';
import { SEED } from '../__tests__/fixtures';
import type { Capabilities } from '../api/capabilities.types';
import { CORE_API_VERSION } from '../version';

import { runScript, type RunContext, type ScriptRecord } from './runScript';
import type { RuntimeDescriptor } from './targetCheck';

// Identity transpile — every test feeds plain JS so the body is the
// transpiled form. Exercising the real Babel/Bun transpilers belongs in
// the host integration tests, not here.
const identityTranspile = async (src: string) => src;

const baseRuntime: RuntimeDescriptor = {
  mountedTargets: ['core'],
  coreApiVersion: CORE_API_VERSION,
};

const baseCaps: Capabilities = {
  accounts: ['mybybit'],
  writes: [],
  idempotent: false,
  storage: { enabled: true, quotaBytes: 102400 },
};

const buildCtx = (overrides: Partial<RunContext> = {}): RunContext => ({
  adapter: createMockAdapter(SEED),
  transpile: identityTranspile,
  runtime: baseRuntime,
  invokedFrom: 'cli',
  ...overrides,
});

const buildScript = (overrides: Partial<ScriptRecord> = {}): ScriptRecord => ({
  id: 'uuid-1',
  name: 'test-script',
  code: '',
  targets: ['core'],
  coreApiVersion: '1.0.0',
  capabilities: baseCaps,
  ...overrides,
});

describe('runScript — orchestration', () => {
  it('runs a happy-path script that prints and returns', async () => {
    const script = buildScript({
      code: `
        api.io.print('hello', 'info');
        api.io.return('done');
      `,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.output).toEqual([{ text: 'hello', kind: 'info' }]);
    expect(result.returnValue).toBe('done');
    expect(result.error).toBeUndefined();
  });

  it('defaults the print kind to "print" when omitted', async () => {
    const script = buildScript({ code: `api.io.print('hi');` });
    const result = await runScript(script, '', buildCtx());
    expect(result.output).toEqual([{ text: 'hi', kind: 'print' }]);
  });

  it('passes parsed args to the IIFE', async () => {
    const script = buildScript({
      code: `
        if (args.positional[0] !== 'buy') throw new Error('positional[0] wrong: ' + args.positional[0]);
        if (args.flags.reduce !== true) throw new Error('reduce flag wrong');
        if (args.flags.tif !== 'GTC') throw new Error('tif flag wrong');
        api.io.return('ok');
      `,
    });
    const result = await runScript(script, 'buy 1 --reduce --tif=GTC', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('ok');
  });

  it('awaits async read calls through the proxy', async () => {
    const script = buildScript({
      code: `
        const balance = await api.getBalance();
        api.io.return(balance.total);
      `,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe(100_000); // SEED mybybit balance.total
  });

  it('captures multiple print lines in order', async () => {
    const script = buildScript({
      code: `
        api.io.print('a', 'info');
        api.io.print('b', 'warn');
        api.io.print('c', 'error');
      `,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.output).toEqual([
      { text: 'a', kind: 'info' },
      { text: 'b', kind: 'warn' },
      { text: 'c', kind: 'error' },
    ]);
  });

  it('treats the LAST api.io.return call as the return value', async () => {
    const script = buildScript({
      code: `
        api.io.return('first');
        api.io.return('second');
      `,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.returnValue).toBe('second');
  });

  it('returns ok:false with the thrown message on top-level throw', async () => {
    const script = buildScript({
      code: `throw new Error('boom');`,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toBe('boom');
    expect(result.output).toEqual([]);
    expect(result.returnValue).toBeUndefined();
  });

  it('surfaces a transpile failure as ok:false instead of crashing the host', async () => {
    // Regression contract: the host-injected transpile primitive is allowed
    // to throw on bad source. The dispatcher must catch it in the outer
    // try/catch and return a normal failure result — anything else turns
    // a user typo into an unhandled rejection that the host doesn't see.
    const failingTranspile = async () => {
      throw new SyntaxError('Unexpected token (1:0)');
    };
    const script = buildScript({ code: '@@@ not real js' });
    const result = await runScript(script, '', buildCtx({ transpile: failingTranspile }));
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/Unexpected token/);
  });
});

describe('runScript — capability + target gates fire before user code', () => {
  it('hard-fails when a declared target is not mounted', async () => {
    const script = buildScript({
      targets: ['core', 'web'],
      // The script body MUST NOT run.
      code: `globalThis.__shouldNotRun = true; throw new Error('should not reach here');`,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/requires the 'web' surface/);
    expect((globalThis as Record<string, unknown>).__shouldNotRun).toBeUndefined();
  });

  it('hard-fails when the runtime is too old for the script', async () => {
    const script = buildScript({
      coreApiVersion: '99.0.0',
      code: 'throw new Error("should not reach here");',
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/CORE v99\.0\.0/);
  });

  it('denies reads not in capabilities.reads', async () => {
    const script = buildScript({
      capabilities: { ...baseCaps, reads: ['getBalance'] },
      code: `await api.getOrderbook('BTCUSDT');`,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/capability denied: read 'getOrderbook'/);
  });

  it('allows reads not in capabilities.reads when reads is undefined (v1 default)', async () => {
    const script = buildScript({
      // No `reads` key — default = all permitted.
      code: `
        const balance = await api.getBalance();
        const ticker = await api.getTicker('BTCUSDT');
        api.io.return(balance.total + ':' + ticker.last);
      `,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('100000:50005');
  });

  it('denies an explicit account outside capabilities.accounts', async () => {
    const script = buildScript({
      code: `await api.getBalance({ account: 'not-allowed' });`,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/account 'not-allowed' not in script capabilities/);
  });

  it('resolves opts.account to the sole-capability account when omitted', async () => {
    const script = buildScript({
      code: `
        const orders = await api.getOrders();
        api.io.return(orders.length);
      `,
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe(1);
  });
});

describe('runScript — meta surface', () => {
  it('exposes scriptId / scriptName / invokedFrom / targets / capabilities', async () => {
    const script = buildScript({
      code: `
        api.io.return({
          id: api.meta.scriptId,
          name: api.meta.scriptName,
          invokedFrom: api.meta.invokedFrom,
          targets: api.meta.targets,
          accounts: api.meta.capabilities.accounts,
          version: api.version,
        });
      `,
    });
    const result = await runScript(script, '', buildCtx({ invokedFrom: 'hotkey' }));
    expect(result.returnValue).toEqual({
      id: 'uuid-1',
      name: 'test-script',
      invokedFrom: 'hotkey',
      targets: ['core'],
      accounts: ['mybybit'],
      version: CORE_API_VERSION,
    });
  });

  it('threads loopSymbol when supplied via context', async () => {
    const script = buildScript({
      code: `api.io.return(api.meta.loopSymbol);`,
    });
    const result = await runScript(script, '', buildCtx({ loopSymbol: 'ETHUSDT' }));
    expect(result.returnValue).toBe('ETHUSDT');
  });
});

describe('runScript — phase-2-step-3 action surface', () => {
  it('routes api.exchange.placeOrder through proxy + adapter when writes-allowlisted', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: { ...baseCaps, writes: ['placeOrder'] },
      code: `
        const r = await api.exchange.placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1 });
        api.io.return(r.orderIds[0]);
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(typeof result.returnValue).toBe('string');
    // Adapter recorded the dispatch with the resolved account injected.
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.writes).toHaveLength(1);
    expect(adapter.logs.writes[0]).toMatchObject({
      kind: 'placeOrder',
      account: 'mybybit',
      opts: { symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1 },
    });
  });

  it('denies writes not in capabilities.writes BEFORE reaching the adapter', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      // writes allowlist is empty by default in baseCaps.
      code: `await api.exchange.placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1 });`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/capability denied: write 'placeOrder'/);
    // Hard rule: denial path must never reach the adapter.
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.writes).toEqual([]);
  });

  it('denies writes against an exchange outside capabilities.exchanges', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        accounts: ['mybybit', 'hl-prod'],
        exchanges: ['hyperliquid'],
        writes: ['placeOrder'],
      },
      // mybybit is on `bybit` — exchange gate must trip.
      code: `await api.exchange.placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1, account: 'mybybit' });`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/exchange 'bybit' not in script capabilities\.exchanges/);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.writes).toEqual([]);
  });

  it('routes api.exchange.runCommand with depth=1 and the verb allowlist', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `await api.exchange.runCommand('buy 1 BTCUSDT');`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands).toEqual([
      { command: 'buy 1 BTCUSDT', account: 'mybybit', symbol: undefined, depth: 1 },
    ]);
  });

  it('does not count concurrent sibling runCommand calls as recursive depth', async () => {
    // The recursion cap protects script -> runCommand -> script nesting.
    // Parallel sibling calls from the same script are all depth=1.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `
        const results = await Promise.allSettled([
          api.exchange.runCommand('buy 1'),
          api.exchange.runCommand('buy 2'),
          api.exchange.runCommand('buy 3'),
          api.exchange.runCommand('buy 4'),
          api.exchange.runCommand('buy 5'),
        ]);
        api.io.return(results.map(r => r.status));
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(result.returnValue).toEqual(['fulfilled', 'fulfilled', 'fulfilled', 'fulfilled', 'fulfilled']);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands.map((c) => c.depth)).toEqual([1, 1, 1, 1, 1]);
  });

  it('increments runCommand depth from the inherited dispatch context', async () => {
    const ctx = buildCtx({ runCommandDepth: 3 });
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `await api.exchange.runCommand('buy 1 BTC');`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands.map((c) => c.depth)).toEqual([4]);
  });

  it('denies runCommand when inherited depth would exceed the recursion cap', async () => {
    const ctx = buildCtx({ runCommandDepth: 4 });
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `await api.exchange.runCommand('buy 1 BTC');`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toBe('capability denied: runCommand recursion depth 5 exceeds cap 4');
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands).toEqual([]);
  });

  it('resets depth between sequential (non-nested) runCommand calls', async () => {
    // Three sequential dispatches — each should bookkeep depth back to 0,
    // so every recorded depth is 1. (Real recursion would show 1/2/3/…)
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `
        await api.exchange.runCommand('buy 1 BTC');
        await api.exchange.runCommand('buy 2 ETH');
        await api.exchange.runCommand('buy 3 SOL');
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands.map((c) => c.depth)).toEqual([1, 1, 1]);
  });

  it('denies runCommand verbs not in the allowlist', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `await api.exchange.runCommand('account export');`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toBe("capability denied: runCommand verb 'account' not in allowlist");
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands).toEqual([]);
  });

  it('fires api.io.print through the adapter and records on the in-memory sink', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      code: `
        api.io.print('hello', 'success');
        api.io.print('plain');
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    // In-memory sink (returned in `result.output`)…
    expect(result.output).toEqual([
      { text: 'hello', kind: 'success' },
      { text: 'plain', kind: 'print' },
    ]);
    // …and the host adapter's print sink ran with the same lines.
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.io).toEqual([
      { text: 'hello', kind: 'success' },
      { text: 'plain', kind: 'print' },
    ]);
  });

  it('storage stubs still throw until Phase 3 wires them', async () => {
    const ctx = buildCtx();
    const script = buildScript({
      code: `await api.storage.get('foo');`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/api\.storage is not wired yet/);
  });

  it('does not leak runCommand depth when validation throws', async () => {
    // Regression: depth used to live in mutable proxy state. A user-code
    // catch around a denied call could leak depth and reject later calls.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        writes: ['runCommand'],
        runCommand: { enabled: true, allowedVerbs: ['buy'] },
      },
      code: `
        try { await api.exchange.runCommand('account export'); } catch (e) {}
        await api.exchange.runCommand('buy 1 BTC');
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.commands.map((c) => c.depth)).toEqual([1]);
  });

  it('denies getCombinedBalance accounts that fall outside capabilities.accounts', async () => {
    // Regression: getCombinedBalance previously forwarded `opts.accounts`
    // straight through, so a script allowlisted for `mybybit` could pull
    // balances for any account name.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: { ...baseCaps, reads: ['getCombinedBalance'] },
      code: `await api.getCombinedBalance({ accounts: ['mybybit', 'someone-elses-acct'] });`,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/account 'someone-elses-acct' not in script capabilities/);
  });

  it('narrows getCombinedBalance to capabilities.accounts when opts.accounts is omitted', async () => {
    // The mock fixture carries `mybybit` (totalEquity 100_000) +
    // `hl-prod` (totalEquity 10_000). When the script is allowlisted for
    // only `mybybit` and asks for "all accounts", the dispatcher must
    // narrow to the allowlist — so the rolled-up totalEquity is 100_000,
    // not 110_000.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        accounts: ['mybybit'],
        reads: ['getCombinedBalance'],
      },
      code: `
        const r = await api.getCombinedBalance();
        api.io.return(r.totalEquity);
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe(100_000);
  });

  it('empty allowlist: getCombinedBalance aggregates ALL streaming accounts (forwards undefined, not [])', async () => {
    // Regression: an empty `accounts: []` means "no restriction" (SPEC §2.6).
    // getCombinedBalance previously forwarded `accounts: []` when the allowlist
    // was empty, which the host reads as "aggregate nothing" → 0. It must
    // forward `undefined` so all streaming accounts roll up (mybybit 100k +
    // hl-prod 10k = 110k).
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: { ...baseCaps, accounts: [], reads: ['getCombinedBalance'] },
      code: `
        const r = await api.getCombinedBalance();
        api.io.return(r.totalEquity);
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe(110_000);
  });

  it('empty allowlist: getHistoricalPosition does not enforce account membership', async () => {
    // Regression: getHistoricalPosition validated the account against the
    // allowlist directly, so an empty allowlist threw for any account instead
    // of treating empty as unrestricted.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: { ...baseCaps, accounts: [], reads: ['getHistoricalPosition'] },
      code: `
        const r = await api.getHistoricalPosition({ account: 'mybybit', symbol: 'BTCUSDT' });
        api.io.return(r ? r.account : 'none');
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(result.error).toBeUndefined();
    expect(result.returnValue).toBe('mybybit');
  });

  it('treats placeOrders([]) as a no-op without crashing on requireAccount', async () => {
    // Regression: the InProcessProxy `placeOrders` case dereferenced
    // `list[0]` to resolve the account, so an empty batch crashed with
    // "no account hint" before reaching the adapter. Empty batches are
    // valid no-ops.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: { ...baseCaps, writes: ['placeOrders'] },
      code: `
        const r = await api.exchange.placeOrders([]);
        api.io.return(r.orderIds);
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(result.returnValue).toEqual([]);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.writes).toEqual([]);
  });

  it('defeats TOCTOU on getCombinedBalance via a custom-iterable opts.accounts', async () => {
    // Regression: `opts.accounts` was iterated once for validation and the
    // SAME reference was forwarded to the adapter. A custom iterator that
    // yields a different value on the second pass can leak balances for
    // accounts outside the allowlist. The fix snapshots via `Array.from`
    // BEFORE the validate loop.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        accounts: ['mybybit'],
        reads: ['getCombinedBalance'],
      },
      code: `
        let pass = 0;
        const sneaky = { [Symbol.iterator]() {
          return { next() {
            pass++;
            // First iteration yields the allowlisted account; second yields
            // a non-allowlisted account. If the dispatcher only validates
            // once and forwards the same iterable, the leak goes through.
            if (pass === 1) return { value: 'mybybit', done: false };
            if (pass === 2) return { value: 'hl-prod',  done: false };
            return { value: undefined, done: true };
          } };
        }};
        try {
          await api.getCombinedBalance({ accounts: sneaky });
          api.io.return('LEAK');
        } catch (e) {
          api.io.return(e.message);
        }
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    // Either the validator rejects 'hl-prod' (preferred) or the snapshot
    // freezes the iterable so the adapter sees only 'mybybit'. Either way
    // the script must not get a result for 'hl-prod'.
    expect(result.returnValue).not.toBe('LEAK');
    expect(result.returnValue).toMatch(/not in script capabilities/);
  });

  it('defeats TOCTOU on getHistoricalPosition via a getter on query.account', async () => {
    // Regression: `query.account` was read once for validation and the
    // SAME query reference was forwarded to the adapter, which read
    // `query.account` again. A getter could return different values on
    // each pass and bypass the allowlist. Fix snapshots the fields into
    // a fresh POJO before validation.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: {
        ...baseCaps,
        accounts: ['mybybit'],
        reads: ['getHistoricalPosition'],
      },
      code: `
        let pass = 0;
        const query = {
          symbol: 'BTCUSDT',
          get account() {
            pass++;
            return pass === 1 ? 'mybybit' : 'hl-prod';
          },
        };
        try {
          const r = await api.getHistoricalPosition(query);
          api.io.return({ leaked: pass > 1, value: r });
        } catch (e) {
          api.io.return({ leaked: false, error: e.message });
        }
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect(result.returnValue).toMatchObject({ leaked: false });
  });

  it('requires both closePosition AND placeOrder writes because adapters synthesize close via placeOrder', async () => {
    // Both real adapters implement closePosition by emitting
    // `placeOrder({ reduceOnly: true })`. A script granted only
    // `writes: ['closePosition']` could otherwise emit market orders
    // through the synthesizer, bypassing the placeOrder allowlist.
    const ctx = buildCtx();
    const scriptCloseOnly = buildScript({
      capabilities: { ...baseCaps, writes: ['closePosition'] },
      code: `await api.exchange.closePosition({ symbol: 'BTCUSDT' });`,
    });
    const r1 = await runScript(scriptCloseOnly, '', ctx);
    expect(r1.ok).toBe(false);
    expect(r1.error).toMatch(/capability denied: write 'placeOrder'/);

    // With BOTH capabilities granted, the call should reach the proxy
    // dispatch (the mock's closePosition still throws, but the gate is
    // now downstream of the capability check we care about).
    const ctxOk = buildCtx();
    const scriptBoth = buildScript({
      capabilities: { ...baseCaps, writes: ['closePosition', 'placeOrder'] },
      code: `await api.exchange.closePosition({ symbol: 'BTCUSDT' });`,
    });
    const r2 = await runScript(scriptBoth, '', ctxOk);
    expect(r2.ok).toBe(true);
    const adapter = ctxOk.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.writes.at(-1)).toMatchObject({ kind: 'closePosition' });
  });

  it('rejects api.meta.capabilities mutation attempts so scripts cannot expand their own allowlist', async () => {
    // Regression: `Object.freeze(record.capabilities)` was shallow, so
    // `api.meta.capabilities.writes.push(...)` mutated the dispatcher's
    // working copy and let user code self-grant capabilities.
    const ctx = buildCtx();
    const script = buildScript({
      capabilities: { ...baseCaps, writes: [] },
      code: `
        let pushThrew = false;
        try { api.meta.capabilities.writes.push('placeOrder'); }
        catch (e) { pushThrew = true; }
        // Whether the push throws (strict mode) or silently no-ops, the
        // dispatcher must still deny the action.
        let denied = false;
        try {
          await api.exchange.placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1 });
        } catch (e) {
          denied = (e instanceof Error) && /capability denied: write 'placeOrder'/.test(e.message);
        }
        api.io.return({ pushThrew, denied });
      `,
    });
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(true);
    expect((result.returnValue as { denied: boolean }).denied).toBe(true);
  });
});

describe('runScript — fn-ref path (SPEC §5.1 file-loader)', () => {
  it('calls record.fn directly when set, skipping transpile + new Function', async () => {
    // Identity transpile would throw if called — proves the fn-path
    // doesn't go through it.
    const throwingTranspile = async () => {
      throw new Error('transpile must not run for the fn-ref path');
    };
    const script = buildScript({
      code: undefined,
      fn: async (_args, api) => {
        api.io.print('from-fn', 'info');
        api.io.return('fn-ok');
      },
    });
    const result = await runScript(script, '', buildCtx({ transpile: throwingTranspile }));
    expect(result.ok).toBe(true);
    expect(result.output).toEqual([{ text: 'from-fn', kind: 'info' }]);
    expect(result.returnValue).toBe('fn-ok');
  });

  it('fires all capability gates against the fn-ref path identically to the code path', async () => {
    const script = buildScript({
      code: undefined,
      // writes allowlist is empty by default in baseCaps — placeOrder must
      // throw before reaching the adapter, regardless of how the script
      // was loaded.
      fn: async (_args, api) => {
        await api.exchange.placeOrder({ symbol: 'BTCUSDT', side: 'buy', type: 'market', amount: 1 });
      },
    });
    const ctx = buildCtx();
    const result = await runScript(script, '', ctx);
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/capability denied: write 'placeOrder'/);
    const adapter = ctx.adapter as ReturnType<typeof import('../__tests__/mockAdapter').createMockAdapter>;
    expect(adapter.logs.writes).toEqual([]);
  });

  it('routes parsed args + meta to the fn the same way the code path does', async () => {
    const script = buildScript({
      code: undefined,
      fn: async (args, api) => {
        api.io.return({
          positional0: args.positional[0],
          flagReduce: args.flags.reduce,
          scriptName: api.meta.scriptName,
        });
      },
    });
    const result = await runScript(script, 'buy --reduce', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.returnValue).toEqual({
      positional0: 'buy',
      flagReduce: true,
      scriptName: 'test-script',
    });
  });

  it('surfaces a fn-thrown error as ok:false', async () => {
    const script = buildScript({
      code: undefined,
      fn: async () => {
        throw new Error('user-fn boom');
      },
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toBe('user-fn boom');
  });

  it('prefers fn over code when BOTH are set', async () => {
    const script = buildScript({
      code: `api.io.return('from-code');`,
      fn: async (_args, api) => {
        api.io.return('from-fn');
      },
    });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(true);
    expect(result.returnValue).toBe('from-fn');
  });

  it('fails clearly when neither code nor fn is set', async () => {
    const script = buildScript({ code: undefined });
    const result = await runScript(script, '', buildCtx());
    expect(result.ok).toBe(false);
    expect(result.error).toMatch(/neither code nor fn is set/);
  });
});
