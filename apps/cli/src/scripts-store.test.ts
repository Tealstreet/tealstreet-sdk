import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

import { afterEach, beforeEach, describe, expect, it } from 'vitest';

// The loader derives `~/.tealstreet/scripts/` from `os.homedir()`,
// which on POSIX reads `HOME` and on Windows reads `USERPROFILE`.
// Redirect both BEFORE importing the module under test so the real
// `~/.tealstreet/scripts/` is never touched.
const TMP_BASE = fs.mkdtempSync(path.join(os.tmpdir(), 'tealstreet-scripts-test-'));
process.env.HOME = TMP_BASE;
process.env.USERPROFILE = TMP_BASE;

// Now safe to import — the module-scope constants resolve under TMP_BASE.
const { awaitScriptsLoaded, clearScriptsCache, ensureScriptsDir, listScripts, loadScripts, lookupScript, SCRIPTS_DIR } =
  await import('./scripts-store');

const writeScript = (name: string, contents: string): string => {
  if (!fs.existsSync(SCRIPTS_DIR)) {
    fs.mkdirSync(SCRIPTS_DIR, { recursive: true });
  }
  const file = path.join(SCRIPTS_DIR, `${name}.ts`);
  fs.writeFileSync(file, contents);
  return file;
};

// `loadScripts()` re-seeds `hello.ts` when it calls `ensureScriptsDir()`
// internally, so the seeded example always shows up in the result.
// Drop it from assertions so each test only reasons about the scripts
// it wrote itself.
const withoutHello = <T extends { readonly name: string }>(scripts: readonly T[]): T[] =>
  scripts.filter((s) => s.name !== 'hello');

const validScript = (returnValue = 'ok') => `
import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';

export const meta: ScriptMeta = {
  targets: ['core', 'cli'],
  capabilities: {
    accounts: ['mybybit'],
    writes: [],
    idempotent: true,
    storage: { enabled: false, quotaBytes: 0 },
  },
};

export default async function (_args: ScriptArgs, api: CoreApi): Promise<void> {
  api.io.return(${JSON.stringify(returnValue)});
}
`;

beforeEach(() => {
  // Fresh dir for every test. `ensureScriptsDir`/`loadScripts` re-seed
  // `hello.ts` + `types.d.ts` + `tsconfig.json` — tests filter `hello`
  // out via `withoutHello`. Tests that need to write a script before
  // `ensureScriptsDir` runs go through `writeScript`, which mkdirs the
  // dir lazily.
  if (fs.existsSync(SCRIPTS_DIR)) {
    fs.rmSync(SCRIPTS_DIR, { recursive: true, force: true });
  }
  clearScriptsCache();
});

afterEach(() => {
  // Don't leave the seeded dir lying around — keeps test runs hermetic
  // across re-orders.
  if (fs.existsSync(SCRIPTS_DIR)) {
    fs.rmSync(SCRIPTS_DIR, { recursive: true, force: true });
  }
});

describe('ensureScriptsDir — seeds templates on first launch', () => {
  it('creates the dir + writes tsconfig.json, types.d.ts, and hello.ts', () => {
    expect(fs.existsSync(SCRIPTS_DIR)).toBe(false);
    ensureScriptsDir();
    expect(fs.existsSync(SCRIPTS_DIR)).toBe(true);
    expect(fs.existsSync(path.join(SCRIPTS_DIR, 'tsconfig.json'))).toBe(true);
    expect(fs.existsSync(path.join(SCRIPTS_DIR, 'types.d.ts'))).toBe(true);
    expect(fs.existsSync(path.join(SCRIPTS_DIR, 'hello.ts'))).toBe(true);
  });

  it('preserves user edits to tsconfig.json + hello.ts on subsequent launches', () => {
    ensureScriptsDir();
    fs.writeFileSync(path.join(SCRIPTS_DIR, 'tsconfig.json'), '{ "userEdit": true }');
    fs.writeFileSync(path.join(SCRIPTS_DIR, 'hello.ts'), '// user-edited');
    ensureScriptsDir();
    expect(fs.readFileSync(path.join(SCRIPTS_DIR, 'tsconfig.json'), 'utf-8')).toBe('{ "userEdit": true }');
    expect(fs.readFileSync(path.join(SCRIPTS_DIR, 'hello.ts'), 'utf-8')).toBe('// user-edited');
  });

  it('regenerates types.d.ts on every launch (must stay in sync with CLI version)', () => {
    ensureScriptsDir();
    fs.writeFileSync(path.join(SCRIPTS_DIR, 'types.d.ts'), '// stale user edit');
    ensureScriptsDir();
    const refreshed = fs.readFileSync(path.join(SCRIPTS_DIR, 'types.d.ts'), 'utf-8');
    expect(refreshed).not.toBe('// stale user edit');
    expect(refreshed).toMatch(/AUTO-GENERATED on every CLI launch/);
  });
});

describe('loadScripts — happy paths', () => {
  it('loads a valid script and exposes it via listScripts + lookupScript', async () => {
    writeScript('myStrategy', validScript('done'));

    const loaded = withoutHello(await loadScripts());
    expect(loaded).toHaveLength(1);
    expect(loaded[0]?.name).toBe('myStrategy');
    expect(loaded[0]?.id).toBe('myStrategy');
    expect(loaded[0]?.targets).toEqual(['core', 'cli']);
    expect(loaded[0]?.capabilities.accounts).toEqual(['mybybit']);
    expect(typeof loaded[0]?.fn).toBe('function');

    expect(withoutHello(listScripts())).toHaveLength(1);
    expect(lookupScript('myStrategy')?.name).toBe('myStrategy');
    expect(lookupScript('nope')).toBeUndefined();
  });

  it('picks up edits on reload via the cache-buster query', async () => {
    writeScript('greeter', validScript('v1'));
    await loadScripts();
    const first = lookupScript('greeter');
    expect(first).toBeDefined();

    // Mutate the file in place; without cache-bust the loader would
    // return the cached module.
    writeScript('greeter', validScript('v2'));
    await loadScripts();

    // The fn reference must point at the rewritten module — easiest
    // way to assert that is to call it with a mock api and watch the
    // return-slot get 'v2'.
    let captured: unknown;
    const fakeApi = {
      io: {
        return: (v: unknown) => {
          captured = v;
        },
        print: () => {},
      },
    };
    const fakeArgs = { positional: [], flags: {}, raw: '' };
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await lookupScript('greeter')?.fn?.(fakeArgs as any, fakeApi as any);
    expect(captured).toBe('v2');
  });
});

describe('loadScripts — rejection paths', () => {
  it('loads files whose name collides with REPL builtins — prefix-only invocation makes it safe', async () => {
    // Phase 5 ships `script:<name>` as the only invocation path, so
    // filenames like `buy.ts` / `tasks.ts` / `set.ts` are legal — they
    // collide with no possible REPL command (the prefix is unambiguous).
    // This was the explicit reframe in SPEC §6.1.
    writeScript('buy', validScript());
    writeScript('tasks', validScript());
    writeScript('set', validScript());

    const loaded = withoutHello(await loadScripts());
    expect(loaded.map((s) => s.name).sort()).toEqual(['buy', 'set', 'tasks']);
  });

  it('skips files with invalid names (matching warning, not throwing)', async () => {
    // Make sure the dir exists before the test mutates it (loadScripts
    // will re-seed if necessary).
    ensureScriptsDir();
    // Invalid: starts with a digit.
    writeScript('123start', validScript());
    // Invalid: contains a dot in the name segment.
    fs.writeFileSync(path.join(SCRIPTS_DIR, 'my.bad.ts'), validScript());

    const loaded = withoutHello(await loadScripts());
    expect(loaded).toEqual([]);
  });

  it('skips subdirectories', async () => {
    ensureScriptsDir();
    fs.mkdirSync(path.join(SCRIPTS_DIR, 'nested'));
    fs.writeFileSync(path.join(SCRIPTS_DIR, 'nested', 'inner.ts'), validScript());

    const loaded = withoutHello(await loadScripts());
    expect(loaded).toEqual([]);
  });

  it('skips .d.ts and hidden files', async () => {
    ensureScriptsDir();
    // types.d.ts is the seeded ambient-declaration file; must never be
    // picked up as a script. Hidden files (e.g. editor swap files) also
    // skipped.
    writeScript('.hidden', validScript());
    expect(fs.existsSync(path.join(SCRIPTS_DIR, 'types.d.ts'))).toBe(true);

    const loaded = withoutHello(await loadScripts());
    expect(loaded).toEqual([]);
  });

  it('isolates a malformed script (missing meta) from valid ones', async () => {
    writeScript('good', validScript('all-good'));
    // Missing `export const meta`.
    writeScript('broken', `export default async function () {}`);

    const loaded = withoutHello(await loadScripts());
    expect(loaded).toHaveLength(1);
    expect(loaded[0]?.name).toBe('good');
  });

  it('isolates a malformed script (missing default export)', async () => {
    writeScript('good', validScript());
    writeScript(
      'noDefault',
      `
import type { ScriptMeta } from '@tealstreet/script-api-core';
export const meta: ScriptMeta = {
  targets: ['core', 'cli'],
  capabilities: { accounts: [], writes: [], idempotent: true, storage: { enabled: false, quotaBytes: 0 } },
};
`,
    );

    const loaded = withoutHello(await loadScripts());
    expect(loaded.map((s) => s.name)).toEqual(['good']);
  });
});

describe('awaitScriptsLoaded — Phase 5 cold-REPL race fix', () => {
  it('returns null when loadScripts has never been called', () => {
    // Fresh module-state per `clearScriptsCache` (beforeEach calls it).
    expect(awaitScriptsLoaded()).toBeNull();
  });

  it('publishes the in-flight promise SYNCHRONOUSLY so a concurrent caller can await it', async () => {
    writeScript('greeter', validScript('hi'));

    // Mimic the dispatcher race: kick off loadScripts (fire-and-forget,
    // like `startRepl` does), then IMMEDIATELY ask for the inflight
    // promise from a sibling code path before the first await resolves.
    const _kickoff = loadScripts();
    const inflight = awaitScriptsLoaded();

    // The promise must be the SAME reference, not null, not a fresh one.
    expect(inflight).toBe(_kickoff);
    expect(inflight).not.toBeNull();

    // Wait for it to settle — registry should be populated.
    await inflight;
    expect(lookupScript('greeter')).toBeDefined();
  });

  it('clearScriptsCache resets the inflight slot to null', async () => {
    writeScript('greeter', validScript());
    await loadScripts();
    expect(awaitScriptsLoaded()).not.toBeNull();

    clearScriptsCache();
    expect(awaitScriptsLoaded()).toBeNull();
  });
});

describe('scripts-store — Phase 5 lookup contract for the dispatcher arm', () => {
  it('lookupScript matches the filename verbatim — case-sensitive, no fuzzy match', async () => {
    writeScript('myStrategy', validScript());
    await loadScripts();

    expect(lookupScript('myStrategy')).toBeDefined();
    // cli-core dispatch is case-insensitive but the script registry is
    // CASE-SENSITIVE — the dispatcher arm shouldn't lowercase the
    // name. `MyStrategy` (mixed case) doesn't resolve. Users either
    // pick a consistent case or get a clear "not loaded" error.
    expect(lookupScript('MyStrategy')).toBeUndefined();
    expect(lookupScript('mystrategy')).toBeUndefined();
  });

  it('lookupScript returns undefined for an unknown name (dispatcher reports loudly)', async () => {
    await loadScripts();
    expect(lookupScript('nope')).toBeUndefined();
  });
});
