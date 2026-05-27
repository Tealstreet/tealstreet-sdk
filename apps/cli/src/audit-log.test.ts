import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mkdtempSync, rmSync, appendFileSync, existsSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

vi.mock('./logger', () => ({ logger: { warn: vi.fn() } }));

let dir: string;
let mod: typeof import('./audit-log');

async function loadFresh() {
  vi.resetModules();
  mod = await import('./audit-log');
}

beforeEach(async () => {
  dir = mkdtempSync(join(tmpdir(), 'audit-test-'));
  process.env.TEALSTREET_CONFIG_DIR = dir;
  await loadFresh();
});

afterEach(() => {
  rmSync(dir, { recursive: true, force: true });
  delete process.env.TEALSTREET_CONFIG_DIR;
});

const entry = (over: Partial<import('./audit-log').AuditEntry> = {}) => ({
  ts: new Date().toISOString(),
  account: 'acct',
  command: 'buy $100',
  ok: true,
  ...over,
});

describe('audit-log', () => {
  it('is off by default', () => {
    expect(mod.isAuditEnabled()).toBe(false);
  });

  it('appendAuditEntry is a no-op while off', () => {
    mod.appendAuditEntry(entry());
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('records entries once enabled', () => {
    mod.setAuditEnabled(true);
    expect(mod.isAuditEnabled()).toBe(true);
    mod.appendAuditEntry(entry({ command: 'buy $100', orderIds: ['o1'] }));
    mod.appendAuditEntry(entry({ command: 'sell $50', ok: false, error: 'rejected' }));
    const tail = mod.readAuditTail(10);
    expect(tail).toHaveLength(2);
    expect(tail[0]).toMatchObject({ command: 'buy $100', ok: true, orderIds: ['o1'] });
    expect(tail[1]).toMatchObject({ command: 'sell $50', ok: false, error: 'rejected' });
  });

  it('persists the enabled flag across a reload', async () => {
    mod.setAuditEnabled(true);
    await loadFresh(); // simulate a CLI restart
    expect(mod.isAuditEnabled()).toBe(true);
  });

  it('appends (never truncates) across reloads', async () => {
    mod.setAuditEnabled(true);
    mod.appendAuditEntry(entry({ command: 'buy 1' }));
    await loadFresh();
    mod.appendAuditEntry(entry({ command: 'buy 2' }));
    const tail = mod.readAuditTail(10);
    expect(tail.map((e) => e.command)).toEqual(['buy 1', 'buy 2']);
  });

  it('tail returns the last N entries, newest last', () => {
    mod.setAuditEnabled(true);
    for (let i = 0; i < 5; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    expect(mod.readAuditTail(2).map((e) => e.command)).toEqual(['buy 3', 'buy 4']);
  });

  it('skips malformed lines in the log', () => {
    mod.setAuditEnabled(true);
    mod.appendAuditEntry(entry({ command: 'buy 1' }));
    appendFileSync(mod.AUDIT_LOG_PATH, 'not json\n', 'utf-8');
    mod.appendAuditEntry(entry({ command: 'buy 2' }));
    expect(mod.readAuditTail(10).map((e) => e.command)).toEqual(['buy 1', 'buy 2']);
  });

  it('tail returns the last N VALID entries despite trailing malformed lines', () => {
    mod.setAuditEnabled(true);
    mod.appendAuditEntry(entry({ command: 'buy 1' }));
    mod.appendAuditEntry(entry({ command: 'buy 2' }));
    appendFileSync(mod.AUDIT_LOG_PATH, 'garbage\ngarbage\n', 'utf-8');
    expect(mod.readAuditTail(2).map((e) => e.command)).toEqual(['buy 1', 'buy 2']);
  });

  it('readAuditTail clamps a non-finite n to the default', () => {
    mod.setAuditEnabled(true);
    for (let i = 0; i < 30; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    expect(mod.readAuditTail(NaN as unknown as number)).toHaveLength(20);
  });
});

describe('recordWriteOp', () => {
  const handled = (over: Record<string, unknown> = {}) => ({ handled: true, success: true, ...over });

  it('is a no-op while auditing is off', () => {
    mod.recordWriteOp('buy $100', 'acct', handled());
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('records a handled write-op head when on', () => {
    mod.setAuditEnabled(true);
    mod.recordWriteOp('buy $100 BTCUSDT', 'acct', handled({ data: { orderIds: ['o1'] } }));
    const tail = mod.readAuditTail(10);
    expect(tail).toHaveLength(1);
    expect(tail[0]).toMatchObject({ command: 'buy $100 BTCUSDT', account: 'acct', ok: true, orderIds: ['o1'] });
  });

  it('records the `mv` alias (alias parity with `move`)', () => {
    mod.setAuditEnabled(true);
    mod.recordWriteOp('mv 50000', 'acct', handled());
    expect(mod.readAuditTail(10).map((e) => e.command)).toEqual(['mv 50000']);
  });

  it('does NOT record read commands', () => {
    mod.setAuditEnabled(true);
    mod.recordWriteOp('positions', 'acct', handled());
    mod.recordWriteOp('orders', 'acct', handled());
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('records a handled FAILURE with ok:false + error', () => {
    mod.setAuditEnabled(true);
    mod.recordWriteOp('buy $100', 'acct', { handled: true, success: false, error: 'rejected' });
    const tail = mod.readAuditTail(10);
    expect(tail[0]).toMatchObject({ ok: false, error: 'rejected' });
  });

  it('is a no-op for an unhandled command', () => {
    mod.setAuditEnabled(true);
    mod.recordWriteOp('buy $100', 'acct', { handled: false });
    expect(mod.readAuditTail(10)).toEqual([]);
  });
});

describe('recordScriptWrite', () => {
  it('is a no-op while auditing is off', () => {
    mod.recordScriptWrite('acct', 'placeOrder BTCUSDT buy 0.1');
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('records a script-internal write with the [script] prefix + order IDs', () => {
    mod.setAuditEnabled(true);
    mod.recordScriptWrite('acct', 'placeOrder BTCUSDT buy 0.1', { orderIds: ['o1'] });
    const tail = mod.readAuditTail(10);
    expect(tail).toHaveLength(1);
    expect(tail[0]).toMatchObject({
      command: '[script] placeOrder BTCUSDT buy 0.1',
      account: 'acct',
      ok: true,
      orderIds: ['o1'],
    });
  });

  it('records a failed script write with ok:false + error', () => {
    mod.setAuditEnabled(true);
    mod.recordScriptWrite('acct', 'run myStrat 1 BTC', { ok: false, error: 'boom' });
    expect(mod.readAuditTail(10)[0]).toMatchObject({
      command: '[script] run myStrat 1 BTC',
      ok: false,
      error: 'boom',
    });
  });
});

describe('command lineage (runId)', () => {
  it('newRunId returns distinct 8-char hex ids', () => {
    const a = mod.newRunId();
    const b = mod.newRunId();
    expect(a).toMatch(/^[0-9a-f]{8}$/);
    expect(a).not.toBe(b);
  });

  it('omits runId outside a run scope', () => {
    mod.setAuditEnabled(true);
    mod.appendAuditEntry(entry({ command: 'buy 1' }));
    expect(mod.readAuditTail(10)[0].runId).toBeUndefined();
  });

  it('stamps every entry in a run scope with the same runId', () => {
    mod.setAuditEnabled(true);
    mod.runWithAuditRunId('abc12345', () => {
      mod.recordWriteOp('buy $100', 'acct', { handled: true, success: true });
      mod.recordScriptWrite('acct', 'placeOrder BTCUSDT buy 0.1', { orderIds: ['o1'] });
    });
    const tail = mod.readAuditTail(10);
    expect(tail.map((e) => e.runId)).toEqual(['abc12345', 'abc12345']);
  });

  it('keeps separate runs on separate ids', () => {
    mod.setAuditEnabled(true);
    mod.runWithAuditRunId('run00001', () => mod.recordWriteOp('buy 1', 'acct', { handled: true, success: true }));
    mod.runWithAuditRunId('run00002', () => mod.recordWriteOp('buy 2', 'acct', { handled: true, success: true }));
    const tail = mod.readAuditTail(10);
    expect(tail.map((e) => e.runId)).toEqual(['run00001', 'run00002']);
  });

  it('propagates the runId into async continuations spawned inside the scope', async () => {
    mod.setAuditEnabled(true);
    let pending: Promise<void> | undefined;
    mod.runWithAuditRunId('async001', () => {
      // Fire-and-forget: settles AFTER runWithAuditRunId returns, mirroring
      // the chain runner's `&`/default steps.
      pending = Promise.resolve().then(() => {
        mod.recordWriteOp('buy $100', 'acct', { handled: true, success: true });
      });
    });
    await pending;
    expect(mod.readAuditTail(10)[0].runId).toBe('async001');
  });

  it('honors an explicit runId on the entry over the ambient scope', () => {
    mod.setAuditEnabled(true);
    mod.runWithAuditRunId('ambient0', () => {
      mod.appendAuditEntry(entry({ command: 'buy 1', runId: 'explicit' }));
    });
    expect(mod.readAuditTail(10)[0].runId).toBe('explicit');
  });
});

describe('recordFill (attribution)', () => {
  it('is a no-op while auditing is off', () => {
    mod.recordFill('o1', 'Bought 0.1 BTC @ $50,000');
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('skips a fill that maps to no tracked order', () => {
    mod.setAuditEnabled(true);
    mod.recordFill('unknown', 'Bought 0.1 BTC @ $50,000');
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('skips when the fill has no order id', () => {
    mod.setAuditEnabled(true);
    mod.recordFill(undefined, 'Bought 0.1 BTC @ $50,000');
    expect(mod.readAuditTail(10)).toEqual([]);
  });

  it('attributes a fill to the placing command, runId, and account', () => {
    mod.setAuditEnabled(true);
    mod.runWithAuditRunId('runABCD12', () => {
      mod.recordWriteOp('buy $100 BTCUSDT', 'acct', {
        handled: true,
        success: true,
        data: { orderIds: ['o1'] },
      });
    });
    mod.recordFill('o1', 'Bought 0.1 BTC @ $50,000');
    const fill = mod.readAuditTail(10).find((e) => e.command.startsWith('[fill]'));
    expect(fill).toMatchObject({
      runId: 'runABCD12',
      account: 'acct', // from the remembered lineage
      command: '[fill] Bought 0.1 BTC @ $50,000 ← buy $100 BTCUSDT',
      ok: true,
      orderIds: ['o1'],
    });
  });

  it('attributes a fill from a script-internal order write', () => {
    mod.setAuditEnabled(true);
    mod.runWithAuditRunId('scriptRun', () => {
      mod.recordScriptWrite('acct', 'placeOrder BTCUSDT buy 0.1', { orderIds: ['s1'] });
    });
    mod.recordFill('s1', 'Bought 0.1 BTC @ $50,000');
    const fill = mod.readAuditTail(10).find((e) => e.command.startsWith('[fill]'));
    expect(fill).toMatchObject({
      runId: 'scriptRun',
      command: '[fill] Bought 0.1 BTC @ $50,000 ← [script] placeOrder BTCUSDT buy 0.1',
    });
  });

  it('omits runId when the placing order had no run scope', () => {
    mod.setAuditEnabled(true);
    mod.recordWriteOp('buy $100', 'acct', { handled: true, success: true, data: { orderIds: ['o2'] } });
    mod.recordFill('o2', 'Bought 0.1 BTC @ $50,000');
    const fill = mod.readAuditTail(10).find((e) => e.command.startsWith('[fill]'));
    expect(fill).toBeDefined();
    expect(fill!.runId).toBeUndefined();
  });

  it('does NOT inherit an unrelated active run scope when the lineage has no runId', () => {
    // A fill that fires synchronously inside a DIFFERENT command's run must
    // not be mis-stamped with that run's id (the explicit-key opt-out in
    // appendAuditEntry guards this).
    mod.setAuditEnabled(true);
    mod.recordWriteOp('buy $100', 'acct', { handled: true, success: true, data: { orderIds: ['o3'] } });
    mod.runWithAuditRunId('otherRun', () => {
      mod.recordFill('o3', 'Bought 0.1 BTC @ $50,000');
    });
    const fill = mod.readAuditTail(10).find((e) => e.command.startsWith('[fill]'));
    expect(fill!.runId).toBeUndefined();
  });
});

describe('rotation + bounded read', () => {
  it('tails the last N via read-from-end without rotation', () => {
    mod.setAuditEnabled(true);
    for (let i = 0; i < 50; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    expect(mod.readAuditTail(3).map((e) => e.command)).toEqual(['buy 47', 'buy 48', 'buy 49']);
    expect(existsSync(mod.AUDIT_LOG_PATH + '.1')).toBe(false);
  });

  it('rotates the live log to audit.log.1 once it passes the cap', () => {
    mod.setAuditEnabled(true);
    mod.__setMaxLogBytesForTest(200); // ~2 entries per file
    for (let i = 0; i < 20; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    expect(existsSync(mod.AUDIT_LOG_PATH + '.1')).toBe(true);
  });

  it('still returns a correct, contiguous recent window across a rotation', () => {
    mod.setAuditEnabled(true);
    mod.__setMaxLogBytesForTest(200);
    for (let i = 0; i < 20; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    const all = mod.readAuditTail(1000).map((e) => e.command);
    // Newest survives, and the recovered window is a gapless ascending suffix
    // (read-from-end of the live log, then dipping into the backup).
    expect(all[all.length - 1]).toBe('buy 19');
    const nums = all.map((c) => Number(c.split(' ')[1]));
    for (let i = 1; i < nums.length; i++) expect(nums[i]).toBe(nums[i - 1] + 1);
  });

  it('dips into the rotated backup when the live log is short after a rotation', () => {
    mod.setAuditEnabled(true);
    // Cap that fits ~3 entries: a rotation leaves the live log with fewer
    // than the requested tail, so the remainder must come from audit.log.1.
    mod.__setMaxLogBytesForTest(300);
    for (let i = 0; i < 8; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    expect(existsSync(mod.AUDIT_LOG_PATH + '.1')).toBe(true);
    const tail = mod.readAuditTail(4).map((e) => e.command);
    expect(tail).toEqual(['buy 4', 'buy 5', 'buy 6', 'buy 7']);
  });

  it('widens the window so malformed tail lines do not hide older valid entries', () => {
    mod.setAuditEnabled(true);
    for (let i = 0; i < 30; i++) mod.appendAuditEntry(entry({ command: `buy ${i}` }));
    // A burst of malformed/truncated lines at the very end (e.g. a crash
    // mid-write) must not shrink the tail below the requested count.
    for (let i = 0; i < 25; i++) appendFileSync(mod.AUDIT_LOG_PATH, 'garbage line\n', 'utf-8');
    expect(mod.readAuditTail(5).map((e) => e.command)).toEqual(['buy 25', 'buy 26', 'buy 27', 'buy 28', 'buy 29']);
  });
});
