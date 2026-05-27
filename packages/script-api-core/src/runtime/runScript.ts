/**
 * Canonical script entry point. Every invocation surface — CLI REPL
 * command, hotkey, webhook fire, inline web button — dispatches into
 * `runScript`. See SPEC §6.
 *
 * **Transpile is host-provided.** `RunContext.transpile: (src) => Promise<string>`
 * is required at every call. `script-api-core` does NOT bundle Babel —
 * standalone CLI wires Bun's native transpiler, web wires the lazy
 * `@babel/standalone` loader (`apps/web/src/components/customModules/babelLoader.ts`),
 * Phase 8's worker boundary will bring its own. The contract is "string in,
 * string out, ES-ready"; everything inside `script-api-core` treats the
 * transpiled blob opaquely.
 *
 * **Capture-writeback lives in the host.** `runScript` returns the raw
 * `returnValue` the script handed to `api.io.return(...)`. The host
 * (apps/cli, apps/web) runs `normalizeReturnForCapture` and calls into
 * its `PreprocessStore.setVar(...)` — keeps `script-api-core` free of any
 * cli-core dep per SPEC §9.4.
 *
 * Phase 2 steps land here in pieces:
 *   - Step 2 (this file): target validation, args parsing, host-injected
 *     transpile, IIFE eval, await-the-Promise + return shape.
 *   - Step 3 wires real `api.exchange.*` + `api.io` adapter routing.
 *
 * Error handling per SPEC §7.3: top-level throws are caught, surfaced as
 * `RunScriptResult.error`, and the result is marked `ok: false`. The
 * `handled && success === false` contract is the caller's job — this
 * function just hands back the failure cleanly.
 *
 * The function is a pure orchestrator: same script + same args + same
 * adapter state == same return. No retained state between invocations.
 */
import type { Capabilities, TargetName } from '../api/capabilities.types';
import type { CoreApi, CoreMeta, InvokedFrom, PrintKind } from '../api/CoreApi.types';
import type { ScriptArgs } from '../api/args.types';
import type { CoreAdapter } from '../adapter/AdapterContext';
import { InProcessProxy } from '../proxy/InProcessProxy';
import type { ApiProxy } from '../proxy/ApiProxy';
import { parseScriptArgs } from './argParser';
import { buildApi, createScriptIoSink } from './buildApi';
import { type RuntimeDescriptor, validateTargets } from './targetCheck';
import { stripScriptTypeImports, type TranspileFn, wrapScriptBody } from './transpile';

/**
 * Function-reference shape for the file-loader path (SPEC §5.1 Phase 3).
 * The standalone CLI dynamic-imports `.ts` script files; the default
 * export matches this signature. When a `ScriptRecord` carries `fn`,
 * `runScript` calls it directly and skips transpile + IIFE.
 */
export type ScriptFn = (args: ScriptArgs, api: CoreApi) => Promise<unknown> | unknown;

/**
 * What every script-aware host writes when it persists a script.
 * Mirrors the JSON shape in SPEC §2.5.
 *
 * Two paths the runtime accepts:
 *
 *   - **`code`** (legacy / web / worker path): the user-authored source
 *     as a string. `runScript` transpiles + wraps in an async IIFE +
 *     evals via `new Function`. Required when `fn` is absent.
 *   - **`fn`** (CLI file-loader path, SPEC §5.1 Phase 3): a function
 *     reference the host obtained by dynamic-importing the user's
 *     `.ts` file. `runScript` calls it directly. Skips transpile, no
 *     `new Function`. All capability / target / account / runCommand-depth
 *     gates still fire identically.
 *
 * At least one of `code` / `fn` MUST be set. When both are set, `fn`
 * wins — the code field is informational (the loader keeps it for
 * surfaces that want to display source, e.g. a future "view script"
 * UI).
 */
export interface ScriptRecord {
  readonly id: string;
  readonly name: string;
  readonly code?: string;
  readonly fn?: ScriptFn;
  readonly targets: readonly TargetName[];
  readonly coreApiVersion?: string;
  readonly cliApiVersion?: string;
  readonly webApiVersion?: string;
  readonly mobileApiVersion?: string;
  readonly capabilities: Capabilities;
}

/**
 * Host-provided context for a single script invocation. Built fresh each
 * call — no shared state between scripts.
 *
 * `proxy` is optional: when omitted, the dispatcher wraps the adapter in
 * a default `InProcessProxy`. Hosts that ship Phase 8's worker boundary
 * pass their own postMessage-backed proxy here.
 */
export interface RunContext {
  readonly adapter: CoreAdapter;
  readonly transpile: TranspileFn;
  readonly runtime: RuntimeDescriptor;
  readonly invokedFrom: InvokedFrom;
  /** Populated when invoked via `loop <variant>: myScript`. See SPEC §7.5. */
  readonly loopSymbol?: string;
  /** Current nested api.exchange.runCommand depth. Top-level scripts omit it. */
  readonly runCommandDepth?: number;
  /** Override the default `InProcessProxy`. Phase 8 wires a postMessage proxy. */
  readonly proxy?: ApiProxy;
}

export interface ScriptOutputLine {
  readonly text: string;
  readonly kind: PrintKind;
}

export interface RunScriptResult {
  /** `false` when the script threw or capability/target validation failed. */
  readonly ok: boolean;
  /** Every line the script emitted via `api.io.print`, in order. */
  readonly output: readonly ScriptOutputLine[];
  /** Last `api.io.return(value)` argument when the script returned cleanly. */
  readonly returnValue?: unknown;
  /** Set when `ok === false`. */
  readonly error?: string;
}

/**
 * Build a script-facing meta object from the record + invocation context.
 * Capabilities are deep-cloned and deep-frozen so a script that introspects
 * them via `api.meta.capabilities` can't mutate the dispatcher's working
 * copy (e.g. `api.meta.capabilities.writes.push('placeOrder')` to expand
 * its own allowlist). The clone is mandatory — `Object.freeze` is shallow,
 * so freezing the original would still leave nested arrays mutable.
 */
const deepFreeze = <T>(value: T): T => {
  if (value === null || typeof value !== 'object') return value;
  if (Object.isFrozen(value)) return value;
  Object.freeze(value);
  for (const key of Object.keys(value as object) as (keyof T)[]) {
    deepFreeze(value[key]);
  }
  return value;
};

const buildMeta = (record: ScriptRecord, ctx: RunContext): CoreMeta => ({
  scriptId: record.id,
  scriptName: record.name,
  invokedFrom: ctx.invokedFrom,
  targets: Object.freeze(record.targets.slice()),
  // structuredClone is sufficient here — Capabilities is plain JSON-shaped
  // and survives the clone without losing methods. Then deep-freeze so
  // arrays + nested objects on the exposed meta are immutable.
  capabilities: deepFreeze(structuredClone(record.capabilities)),
  loopSymbol: ctx.loopSymbol,
});

/**
 * Compile the user-authored body via the host's transpile + the shared
 * `wrapScriptBody` wrapper. Two-step so a host can also call them
 * separately (e.g. for "transpile on save" per SPEC §13 ahead of an
 * eventual worker bootstrap).
 */
const compileScript = async (code: string, transpile: TranspileFn): Promise<string> => {
  const stripped = stripScriptTypeImports(code);
  const transpiled = await transpile(stripped);
  return wrapScriptBody(transpiled);
};

/**
 * Build + execute the async IIFE. The wrapped body is `return (async () => {…})();`,
 * so `new Function('args', 'api', body)(args, api)` is itself the
 * Promise the script returns. Errors raised inside the IIFE are caught
 * by the outer try/catch in `runScript`.
 */
const evalScript = async (wrappedBody: string, args: unknown, api: unknown): Promise<void> => {
  // `new Function` is intentional — SPEC §8.1 endorses in-process eval as
  // the v1 isolation model (same boundary as today's hotkeys). Phase 8's
  // worker swaps this for a CSP'd context.
  const fn = new Function('args', 'api', wrappedBody) as (args: unknown, api: unknown) => Promise<void>;
  await fn(args, api);
};

export const runScript = async (record: ScriptRecord, rawArgs: string, ctx: RunContext): Promise<RunScriptResult> => {
  try {
    // 1. Hard-fail target / version mismatch BEFORE we touch user code.
    validateTargets(record, ctx.runtime);

    // 2. Parse args per SPEC §7.1.
    const args = parseScriptArgs(rawArgs);

    // 3. Build the api object the IIFE receives.
    const sink = createScriptIoSink();
    const proxy = ctx.proxy ?? new InProcessProxy(ctx.adapter);
    const activeAccount = await Promise.resolve(ctx.adapter.getActiveAccount());
    // Pre-resolve account → exchange so `validateExchangeCapability` fires
    // synchronously inside the action surface. The map is best-effort — if
    // the host hasn't hydrated accounts yet, the map is empty and the
    // exchange gate short-circuits (only `capabilities.exchanges` set
    // narrows; empty map matches "trust the account allowlist").
    const accountsList = await Promise.resolve(ctx.adapter.getAccounts());
    const accountToExchange: Record<string, string> = {};
    for (const acct of accountsList) {
      accountToExchange[acct.name] = acct.exchange;
    }

    const api = buildApi({
      proxy,
      capabilities: record.capabilities,
      activeAccount,
      accountToExchange,
      sink,
      meta: buildMeta(record, ctx),
      printingHost: ctx.adapter,
      runCommandDepth: ctx.runCommandDepth,
    });

    // 4. Run the script. Two paths:
    //    (a) `record.fn` set → host already loaded the function reference
    //        (CLI file loader per SPEC §5.1). Skip transpile + new Function.
    //    (b) `record.code` set → transpile via host primitive, wrap in IIFE,
    //        eval via `new Function`. Legacy / web / worker path.
    if (record.fn) {
      await record.fn(args, api);
    } else if (record.code !== undefined) {
      const wrappedBody = await compileScript(record.code, ctx.transpile);
      await evalScript(wrappedBody, args, api);
    } else {
      throw new Error(`script '${record.name}': neither code nor fn is set`);
    }

    return {
      ok: true,
      output: Object.freeze(sink.output.slice()),
      returnValue: sink.hasReturn ? sink.returnValue : undefined,
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, output: [], error: message };
  }
};
