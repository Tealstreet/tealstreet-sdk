/**
 * Drift test for `script-types-bundle.ts` — re-runs the per-package
 * generators in-memory and compares against the on-disk artifact that
 * the CLI binary inlines at compile time.
 *
 * The CLI seeds the assembled bundle as `~/.tealstreet/scripts/
 * types.d.ts` on every launch (see `scripts-templates.ts`). If the
 * committed bundle goes stale, user editors silently load mismatched
 * types. This test is the gate that catches that — failing CI forces a
 * `yarn generate:types` + commit before merge.
 *
 * The generators do real TypeScript work (`dts-bundle-generator` +
 * a full TS program over `safe-cex/src/**`), so the in-memory rerun
 * is non-trivial — give it a generous timeout. The cost is paid once
 * per CI run, which is the right tradeoff for the drift-safety
 * guarantee.
 */
import * as fs from 'fs';
import * as path from 'path';

import { describe, expect, it } from 'vitest';

const ON_DISK_PATH = path.join(__dirname, 'script-types-bundle.ts');

// The orchestrator this drift gate re-runs (`scripts/generate-script-types-bundle.ts`)
// is a monorepo-root build script. It is deliberately NOT part of the OSS
// mirror slice — the mirror is a fast pre-check, while the authoritative drift
// gate runs in the monorepo on sync-back (see docs/safe-cex-cli-mirror-plan.md
// §9). The partial-dynamic specifier below is resolved by Vite as a glob over
// `scripts/`: it matches in the monorepo (gate runs in full) and matches
// nothing in the mirror, where the dir is absent (import rejects → block
// skips). A template specifier also keeps tsc from binding the module, so the
// mirror's typecheck stays clean without the file present.
const generatorName = 'generate-script-types-bundle';
const generator = await import(`../../../scripts/${generatorName}.ts`).catch(() => null);

describe.skipIf(generator === null)('script-types-bundle drift', () => {
  it('on-disk bundle matches what the generators emit right now', () => {
    const onDisk = fs.readFileSync(ON_DISK_PATH, 'utf8');
    const rebuilt = generator.renderBundleFile(generator.buildBundle());
    expect(rebuilt).toBe(onDisk);
  }, 60_000);
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
import { CORE_TYPES_DTS } from './script-types-bundle';

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
