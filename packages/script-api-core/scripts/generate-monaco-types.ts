/**
 * Phase 6 — emit the `script-api-core` surface as a single rollup-
 * style `.d.ts` string, wrapped in `declare module
 * '@tealstreet/script-api-core' { ... }` so the seeded
 * `~/.tealstreet/scripts/types.d.ts` doesn't need a real node_modules
 * tree to resolve `import type { CoreApi, ScriptArgs, ScriptMeta }`.
 *
 * Generator uses `dts-bundle-generator` which:
 *   - Inlines the transitive closure of referenced types (including
 *     `@tealstreet/safe-cex/src/types` re-exports) so user scripts can
 *     reach `Order` / `Position` / `OrderType` etc. without a real
 *     safe-cex install.
 *   - Strips `declare` / `export` prefixes as needed to nest inside a
 *     `declare module '...' { ... }` block.
 *   - Sorts nodes for deterministic output (drift-test stability).
 *
 * Runtime constraint: this generator runs under Bun (via the root
 * `yarn generate:types` script) and under Node (via the vitest drift
 * test). It must use only platform-neutral APIs (`fs`/`path`/`url`)
 * plus the bundler. See SPEC §11 Phase 6.
 */
import * as path from 'path';
import { generateDtsBundle } from 'dts-bundle-generator';

const PACKAGE_ROOT = path.resolve(__dirname, '..');
// Phase 6 v1.1 — entry point split. The generator walks `src/script.ts`
// (user-facing surface only), NOT `src/index.ts` (full surface incl.
// host internals like InProcessProxy / runScript / buildApi). The seeded
// `~/.tealstreet/scripts/types.d.ts` should only expose what user
// scripts can legitimately reach; see `src/script.ts` for the why.
const ENTRY = path.join(PACKAGE_ROOT, 'src', 'script.ts');
// Dedicated bundling tsconfig that broadens `include` to also pull in
// `@tealstreet/safe-cex/src/**/*.ts`. The package's own
// `tsconfig.json` only includes `src/**/*` (typecheck scope), but the
// bundler must trace safe-cex's transitive imports — see
// `bundle.tsconfig.json` in this directory.
const TSCONFIG = path.join(__dirname, 'bundle.tsconfig.json');

/**
 * Emits the raw bundled `.d.ts` body — NO `declare module '<pkg>' {
 * ... }` wrapper. The orchestrator (`scripts/generate-script-types-
 * bundle.ts`) adds the wrap so that the wrap rule lives in one place
 * across all three packages.
 *
 * `umdModuleName` is intentionally NOT passed: it makes
 * dts-bundle-generator emit `export as namespace <name>` with `/`
 * and `@` in the identifier (invalid syntax). The manual wrap in the
 * orchestrator is the documented workaround.
 */
export function generateCoreMonacoTypes(): string {
  const [output] = generateDtsBundle(
    [
      {
        filePath: ENTRY,
        output: {
          sortNodes: true,
          noBanner: true,
          inlineDeclareGlobals: false,
        },
      },
    ],
    {
      preferredConfigPath: TSCONFIG,
    },
  );
  return output;
}

// Allow running directly: `bun run packages/script-api-core/scripts/generate-monaco-types.ts`
if (require.main === module) {
  process.stdout.write(generateCoreMonacoTypes());
}
