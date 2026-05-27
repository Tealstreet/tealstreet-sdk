/**
 * Phase 6 — emit the `script-api-web` surface as a `.d.ts` string.
 * The `declare module '@tealstreet/script-api-web' { ... }` wrap is
 * added by the orchestrator
 * (`scripts/generate-script-types-bundle.ts`), so this generator
 * returns the raw bundled body.
 *
 * Today the package is scaffold-only (only exports `WEB_API_VERSION`);
 * the bundler emits a minimal declaration. Phase 7+ will grow this as
 * web-specific runtime extensions land (UI-token resolvers, DOM
 * hotkey triggers, etc.).
 *
 * See SPEC §11 Phase 6 + the sibling generator in
 * `script-api-core/scripts/generate-monaco-types.ts`.
 */
import * as path from 'path';
import { generateDtsBundle } from 'dts-bundle-generator';

const PACKAGE_ROOT = path.resolve(__dirname, '..');
const ENTRY = path.join(PACKAGE_ROOT, 'src', 'index.ts');
const TSCONFIG = path.join(PACKAGE_ROOT, 'tsconfig.json');

export function generateWebMonacoTypes(): string {
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

if (require.main === module) {
  process.stdout.write(generateWebMonacoTypes());
}
