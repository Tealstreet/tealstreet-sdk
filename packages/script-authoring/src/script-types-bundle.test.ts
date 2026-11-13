/**
 * Drift test for `script-types-bundle.ts` — re-runs the per-package
 * generators in-memory and compares against the committed declarations that
 * the CLI binary inlines at compile time.
 *
 * The CLI seeds the assembled bundle as `~/.tealstreet/scripts/
 * types.d.ts` on every launch (see `scripts-templates.ts`). If the
 * committed bundle goes stale, user editors silently load mismatched
 * types. This gate fails CI until the bundle is regenerated from the
 * current source types and committed alongside them.
 *
 * The generators do real TypeScript work (`dts-bundle-generator` +
 * a full TS program over `safe-cex/src/**`), so the in-memory rerun
 * is non-trivial — give it a generous timeout. CI invokes this check
 * explicitly, independently of package-task caching.
 */
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

import { generateScriptTypesBundle, renderBundleFile } from '../scripts/render-script-types-bundle';
import { CORE_TYPES_DTS } from './script-types-bundle';

const DRIFT_TEST_TIMEOUT_MS = 180_000;

describe('script-types-bundle drift', () => {
  it('declares unknown liquidation prices as nullable in user script typings', () => {
    expect(CORE_TYPES_DTS).toMatch(/readonly liquidationPrice:\s*number\s*\|\s*null;/);
  });

  it(
    'the complete committed file matches the shared generator',
    () => {
      const committed = readFileSync(resolve(__dirname, 'script-types-bundle.ts'), 'utf8');
      expect(committed).toBe(generateScriptTypesBundle());
    },
    DRIFT_TEST_TIMEOUT_MS,
  );

  it('normalizes the terminal empty export with NX whitespace rules', () => {
    const core = 'export interface Example {}\nexport { Example };\nexport  {  };\n';
    const rendered = renderBundleFile(core, 'export interface Web {}\nexport {};\n');
    expect(rendered).toContain(
      JSON.stringify(
        "declare module '@tealstreet/script-api-core' {\nexport interface Example {}\nexport { Example };\n}\n",
      ),
    );
    expect(rendered).toContain(
      JSON.stringify("declare module '@tealstreet/script-api-web' {\nexport interface Web {}\n}\n"),
    );
  });
});

/**
 * Phase 6 v1.1 — `script-api-core`'s user-facing entry (`src/script.ts`)
 * must NEVER re-export the host-only surface (`InProcessProxy`,
 * `runScript`, `buildApi`, capability validators, transpile helpers,
 * etc.). If any of these leak into `CORE_TYPES_DTS` as an actual
 * declaration, novice users autocompleting host symbols in their
 * `~/.tealstreet/scripts/*.ts` would hit `Cannot find module …` at
 * script-load (no installed npm package backs the seeded ambient types
 * — see SPEC §11 Phase 6 v1.1).
 *
 * **Why declaration-shape regex, not substring**. `dts-bundle-generator`
 * preserves JSDoc comments on the declarations it emits. Several
 * user-facing types (e.g. `ScriptMeta` in `capabilities.types.ts`,
 * `CORE_API_VERSION` in `version.ts`) carry doc comments that mention
 * host symbols by name for architectural context. Those textual
 * mentions don't autocomplete and aren't load-bearing for the
 * footgun this test guards against. We only fail on actual TS
 * declarations: `(export )?declare class X`, `function X(`,
 * `interface X`, `const X`, `type X`.
 *
 * The list is curated, not exhaustive — if someone adds a NEW host
 * symbol later, extend this list in the same PR.
 */
const HOST_ONLY_SYMBOLS = [
  'InProcessProxy',
  'ApiProxy',
  'CoreAdapter',
  'MaybePromise',
  'runScript',
  'buildApi',
  'createScriptIoSink',
  'ScriptIoSink',
  'BuildApiInputs',
  'PrintingHost',
  'RunContext',
  'RunScriptResult',
  'ScriptFn',
  'ScriptOutputLine',
  'ScriptRecord',
  'ScriptDescriptor',
  'RuntimeDescriptor',
  'validateTargets',
  'resolveAccount',
  'parseScriptArgs',
  'MAX_RUN_COMMAND_DEPTH',
  'assertIdempotentForRetry',
  'assertWithinStorageQuota',
  'validateExchangeCapability',
  'validateReadCapability',
  'validateRunCommandCapability',
  'validateWriteCapability',
  'stripScriptTypeImports',
  'wrapScriptBody',
  'TranspileFn',
  'normalizeReturnForCapture',
  'CaptureWriteback',
] as const;

const declarationPatterns = (sym: string) => [
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?class\\s+${sym}\\b`),
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?function\\s+${sym}\\b`),
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?interface\\s+${sym}\\b`),
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?const\\s+${sym}\\b`),
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?type\\s+${sym}\\s*=`),
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?enum\\s+${sym}\\b`),
  new RegExp(`(?:export\\s+)?(?:declare\\s+)?namespace\\s+${sym}\\b`),
];

describe('script-types-bundle — host-symbol leak guard (Phase 6 v1.1)', () => {
  it.each(HOST_ONLY_SYMBOLS)('does NOT export %s as a declaration', (sym) => {
    const patterns = declarationPatterns(sym);
    const leaked = patterns.some((p) => p.test(CORE_TYPES_DTS));
    expect(leaked, `${sym} appears as a real declaration in CORE_TYPES_DTS`).toBe(false);
  });

  it('still exports CoreApi, ScriptArgs, ScriptMeta — the user-facing surface', () => {
    expect(CORE_TYPES_DTS).toMatch(/(?:export\s+)?interface\s+CoreApi\b/);
    expect(CORE_TYPES_DTS).toMatch(/(?:export\s+)?interface\s+ScriptArgs\b/);
    expect(CORE_TYPES_DTS).toMatch(/(?:export\s+)?interface\s+ScriptMeta\b/);
  });
});
