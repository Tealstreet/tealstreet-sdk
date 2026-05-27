/**
 * Bun-native `transpile` primitive for the script runtime. See SPEC §13
 * + `script-runner.ts`'s `transpile` field.
 *
 * For the **fn-ref path** (the only one Phase 5 invokes — see SPEC §5.1
 * and `ScriptRecord.fn`), the runtime never calls `transpile` because
 * the script has already been imported as a module and the function
 * reference is in hand. The field is required by `RunContext` but
 * unused at runtime.
 *
 * We still wire a real Bun.Transpiler so:
 *   1. The contract is honored — `runScriptFromHost` always has a
 *      working primitive available.
 *   2. If a future phase introduces source-string CLI scripts (e.g.
 *      ad-hoc inline invocation), the transpile is ready.
 *
 * Under vitest the global `Bun` doesn't exist; the function falls back
 * to an identity transform (the tests only exercise the fn-ref path,
 * which never reaches this code).
 */

let cachedTranspile: ((source: string) => Promise<string>) | null = null;

interface BunGlobal {
  readonly Transpiler: new (opts?: { readonly loader?: 'ts' | 'tsx' | 'js' | 'jsx' }) => {
    transformSync(source: string): string;
  };
}

export function getCliTranspile(): (source: string) => Promise<string> {
  if (cachedTranspile) return cachedTranspile;

  const bun = (globalThis as { Bun?: BunGlobal }).Bun;
  if (bun?.Transpiler) {
    const transpiler = new bun.Transpiler({ loader: 'ts' });
    cachedTranspile = async (source) => transpiler.transformSync(source);
  } else {
    // Vitest / Node fallback — fn-ref scripts never reach this path.
    cachedTranspile = async (source) => source;
  }
  return cachedTranspile;
}
