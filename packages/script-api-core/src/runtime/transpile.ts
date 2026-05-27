/**
 * Transpile contract — host-injected. See SPEC §13 ("transpile on save,
 * store the transpiled JS alongside the source").
 *
 * `script-api-core` doesn't bundle a transpiler. Standalone CLI (Bun) and
 * web (browser) ship Babel via different paths:
 *   - apps/cli wires `Bun.Transpiler` (native, no extra dep).
 *   - apps/web wires `@babel/standalone` via the same lazy loader the
 *     custom-modules path uses (`apps/web/src/components/customModules/babelLoader.ts`).
 *
 * Either way, the runtime calls `ctx.transpile(source)` and treats the
 * return value as ES-ready JavaScript: TypeScript stripped, no
 * type-import statements, no class fields needing downlevel, no JSX
 * (scripts don't render — see SPEC §0 + §2.3).
 *
 * Step 2 also provides `wrapScriptBody(transpiledCode)` which wraps the
 * user's body in the IIFE shape `runScript` evaluates. Wrapping is
 * deterministic and host-agnostic so it lives here, not in transpile.
 */

export type TranspileFn = (source: string) => Promise<string>;

/**
 * Wrap a transpiled script body in an async IIFE that receives `{ args, api }`.
 * The returned string is the input to `new Function('args', 'api', ...)`.
 *
 * Why an explicit wrapper rather than letting the host build it: the IIFE
 * binding (`args`, `api`) is part of the script-api contract, not the
 * host's. Wrapping here means every host evaluates the same shape, and a
 * future codemod (e.g. for the v2 worker) only has one wrapping site.
 *
 * The wrapper is **async** so user code can `await api.getBalance()` at
 * top level. `runScript` awaits the returned Promise per SPEC §7.5.
 */
export const wrapScriptBody = (transpiledCode: string): string => {
  // The outer body is what we pass to `new Function('args', 'api', body)`.
  // It returns the awaited result of an async IIFE so callers can `await`
  // the whole thing as one Promise.
  return `return (async () => {\n${transpiledCode}\n})();`;
};

/**
 * Strip `import type { … } from '…'` and bare `import { … } from 'api-types'`
 * shapes — Babel doesn't understand the latter, and we'd rather catch
 * leftover host-shaped imports here than fail mid-transpile. Matches the
 * single-line shapes the custom-modules path supports today
 * (`apps/web/src/components/customModules/CustomModuleSystem.tsx:108-113`).
 *
 * Multi-line imports aren't supported by design — the script API surface
 * doesn't require them and supporting them across both pipelines would
 * mean duplicating a real parser. If a future script template grows to
 * need them, lift this to a tiny AST step at that point.
 */
export const stripScriptTypeImports = (source: string): string => {
  return source
    .replace(/^import\s+type\s+\{[^}]*\}\s+from\s+["'][^"']+["'];?\s*$/gm, '')
    .replace(/^import\s+\{[^}]*\}\s+from\s+["']api-types["'];?\s*$/gm, '')
    .replace(/^import\s+\{[^}]*\}\s+from\s+["']global-types["'];?\s*$/gm, '');
};
