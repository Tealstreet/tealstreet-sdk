/**
 * Templates seeded into `~/.tealstreet/scripts/` on first CLI launch so
 * users can author scripts in their IDE with full language support.
 * See SPEC §5.1 + §11 Phase 6.
 *
 * `TYPES_TEMPLATE` is rewritten on every launch — its ambient
 * declarations must stay in sync with the script-API surface the CLI
 * ships. v1 (Phase 6) reads the surface from
 * `./script-types-bundle.ts`, a committed artifact emitted by
 * `yarn generate:types` at the repo root. A drift test
 * (`script-types-bundle.test.ts`) re-runs the generators in-memory
 * and fails CI if the on-disk bundle goes stale.
 *
 * `TSCONFIG_TEMPLATE` + `HELLO_TEMPLATE` are only-if-missing — user
 * edits to either survive across launches.
 */

import { CLI_TYPES_DTS, CORE_TYPES_DTS, WEB_TYPES_DTS } from './script-types-bundle';

const TYPES_HEADER = `// AUTO-GENERATED on every CLI launch — do not edit. Edits will be
// overwritten the next time the CLI starts. See SPEC §5.1.
//
// This file declares three modules:
//   - '@tealstreet/script-api-core'  (target-agnostic surface — most scripts)
//   - '@tealstreet/script-api-cli'   (CLI-specific extensions; scaffold today)
//   - '@tealstreet/script-api-web'   (web-app extensions; scaffold today)
//
// Each is bundled from the source package at CLI build time and
// stays in lockstep with the running binary's runtime surface (drift
// test in CI). User scripts \`import type\` from these names:
//
//   import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';
//
// The runtime passes \`args\` + \`api\` as PARAMETERS to your
// default-exported function — they are NOT ambient globals. Take them
// as positional arguments on your default-exported function and you
// get full type completion:
//
//   export default async function (args: ScriptArgs, api: CoreApi) { ... }
`;

/**
 * The three tiers as one string. Single definition on purpose: the MCP server
 * hands this to an agent as `get_api_types`, and `TYPES_TEMPLATE` below seeds
 * the same text into `~/.tealstreet/scripts/types.d.ts`. If the two composed
 * it separately they could silently diverge, and an agent would write against
 * a surface the user's editor doesn't have.
 */
export const SCRIPT_TYPES_BUNDLE = [CORE_TYPES_DTS, CLI_TYPES_DTS, WEB_TYPES_DTS].join('\n');

export const TYPES_TEMPLATE = [TYPES_HEADER, SCRIPT_TYPES_BUNDLE].join('\n');

export const TSCONFIG_TEMPLATE = `{
  "compilerOptions": {
    "target": "ESNext",
    "module": "ESNext",
    "moduleResolution": "bundler",
    "strict": true,
    "noEmit": true,
    "isolatedModules": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "skipLibCheck": true,
    "types": []
  },
  "include": ["./**/*.ts", "./types.d.ts"]
}
`;

export const HELLO_TEMPLATE = `// Example Tealstreet script. Delete or rename freely.
//
// Scripts in this directory are loaded by the CLI on startup and on
// \`script reload\`. Invoke from the REPL via the \`script:\` prefix:
//
//   > script:hello
//   > script:hello arg1 --flag=value
//
// The prefix is REQUIRED — bare \`hello\` falls through to cli-core's
// regular command dispatcher, which only errors as unknown if no
// builtin / alias / capture-var of that name matches.
//
// CONSTRAINTS for the v1 file-loader (SPEC §5.1):
//   - Filename must match /^[a-zA-Z_][a-zA-Z0-9_-]*$/ — letters,
//     digits, underscores, hyphens; no spaces, no dots.
//   - One file per script. Subdirectories are ignored.
//   - Filenames that collide with REPL builtins (\`buy.ts\`,
//     \`tasks.ts\`, etc.) ARE legal — invoke via \`script:buy\` to
//     reach the script unambiguously.
//   - No local imports. (\`import type\` from
//     \`@tealstreet/script-api-core\` is fine — types only.)
//   - The script body lives inside the default-exported async function.
//     \`api\` and \`args\` are passed as parameters by the runtime — take
//     them on your function signature to get full type completion.
//
// All trading-action limits are enforced at the dispatcher — see
// \`./types.d.ts\` for the Capabilities shape.
//
// ⚠️ MODULE-LOAD SIDE EFFECTS ARE NOT SANDBOXED. Anything you write at
// the TOP LEVEL of this file (outside the default function below) runs
// at \`script reload\` AND at every REPL startup, BEFORE capability gates
// apply. \`capabilities\` bounds \`api.*\` calls; it does not bound
// \`require('child_process').exec(...)\`, network calls, or filesystem
// writes you put at the top level. Keep everything that should be
// gated INSIDE the default function below.

import type { CoreApi, ScriptArgs, ScriptMeta } from '@tealstreet/script-api-core';

export const meta: ScriptMeta = {
  targets: ['core', 'cli'],
  capabilities: {
    accounts: [],
    writes: [],
    idempotent: true,
    storage: { enabled: false, quotaBytes: 0 },
  },
};

export default async function (args: ScriptArgs, api: CoreApi): Promise<void> {
  const account = await api.getActiveAccount();
  api.io.print(\`hello from \${account}!\`, 'success');
  api.io.return('ok');
}
`;
