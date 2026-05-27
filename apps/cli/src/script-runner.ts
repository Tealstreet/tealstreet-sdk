/**
 * Standalone host shim around `script-api-core`'s `runScript`. Glue
 * code that the REPL / hotkey / webhook dispatcher will call instead
 * of `runScript` directly. Lives in apps/cli so the cli-core
 * `PreprocessStore` dep stays on the host side — `script-api-core`
 * itself doesn't know about cli-core.
 *
 * Three jobs:
 *   1. Idempotent-retry gate (SPEC §4.3 / §7.5). When the chain layer
 *      re-runs a script under `retry N`, refuse unless the script is
 *      marked `capabilities.idempotent: true`. Throws the exact spec
 *      error string `"script '<name>' is not marked idempotent; cannot retry"`.
 *   2. Build a `RunContext` from host primitives: the existing
 *      standalone `CoreAdapter` + the Bun-native transpile.
 *   3. After `runScript` resolves, apply the §7.4 capture-writeback
 *      table via the host's `PreprocessStore.setVar`.
 *
 * The shim is intentionally not in cli-core — `runScript` doesn't take
 * a PreprocessStore in its ctx (per SPEC §9.4 — script-api-core has
 * zero cli-core dep). The shim is the host's gluing point.
 */
import type { PreprocessStore } from '@tealstreet/cli-core';
import { CLI_API_VERSION } from '@tealstreet/script-api-cli';
import {
  CORE_API_VERSION,
  assertIdempotentForRetry,
  normalizeReturnForCapture,
  runScript,
  type RunContext,
  type RunScriptResult,
  type ScriptRecord,
} from '@tealstreet/script-api-core';

import type { CoreAdapter } from '@tealstreet/script-api-core';
import type { CLIPlatformAdapter, CLISession } from '@tealstreet/cli-core';
import type { ExchangeManager } from './exchange-manager';
import { createStandaloneCoreAdapter } from './script-adapter';

/**
 * Optional retry context the chain layer hands in when the step is
 * being re-run via `retry N`. When `isRetry` is true the idempotent
 * gate fires.
 */
export interface RetryContext {
  readonly isRetry: boolean;
  readonly scriptName: string;
}

/**
 * Optional capture target. When set, the runner writes the script's
 * return value to the named user-var per the §7.4 table; for object
 * returns it also writes the JSON-stringified form to `<name>.json`.
 */
export interface CaptureContext {
  readonly captureTo: string;
  readonly preprocessStore: PreprocessStore;
}

export interface RunScriptFromHostOptions {
  readonly record: ScriptRecord;
  readonly rawArgs: string;
  readonly invokedFrom: RunContext['invokedFrom'];
  readonly loopSymbol?: string;
  readonly retry?: RetryContext;
  readonly capture?: CaptureContext;
}

export interface RunScriptFromHostDeps {
  readonly exchangeManager: ExchangeManager;
  readonly session: CLISession;
  readonly platformAdapter: CLIPlatformAdapter;
  /**
   * Transpile primitive. Bun ships `Bun.Transpiler` natively; web
   * lazy-loads `@babel/standalone`. Each host wires its own here.
   */
  readonly transpile: RunContext['transpile'];
}

// Runtime descriptor reflects the versions THIS HOST actually implements —
// pulled straight from the package version constants. Using the script's
// declared version here would defeat `validateTargets`: it would compare
// the script's required version against itself and pass even when the host
// hasn't shipped that surface.
const CLI_RUNTIME_DESCRIPTOR: RunContext['runtime'] = {
  mountedTargets: ['core', 'cli'],
  coreApiVersion: CORE_API_VERSION,
  cliApiVersion: CLI_API_VERSION,
};

/**
 * Run a script from the host side. Wraps `runScript` with the two
 * concerns that don't belong in core: the idempotent gate and the
 * capture-writeback.
 *
 * Returns the raw `RunScriptResult` for callers that want to surface
 * print transcript / error / returnValue without going through the
 * preprocess store.
 */
export const runScriptFromHost = async (
  deps: RunScriptFromHostDeps,
  options: RunScriptFromHostOptions,
): Promise<RunScriptResult> => {
  const adapter: CoreAdapter = createStandaloneCoreAdapter(deps.exchangeManager, deps.session, deps.platformAdapter);
  return runScriptFromHostWithAdapter({ adapter, transpile: deps.transpile }, options);
};

export interface RunScriptFromHostInnerDeps {
  readonly adapter: CoreAdapter;
  readonly transpile: RunContext['transpile'];
}

/**
 * Lower-level entry: takes an already-built `CoreAdapter` instead of
 * constructing one from `ExchangeManager` + `CLISession`. Used by tests
 * to inject a `MockCoreAdapter`; also the seam Phase 8 will swap for
 * the worker-resident adapter without touching the chain layer.
 */
export const runScriptFromHostWithAdapter = async (
  inner: RunScriptFromHostInnerDeps,
  options: RunScriptFromHostOptions,
): Promise<RunScriptResult> => {
  const { record, rawArgs, invokedFrom, loopSymbol, retry, capture } = options;

  // 1. Idempotent gate — fires only on retry. The throw matches the
  //    exact SPEC §4.3 / §7.5 error string so chain-layer callers can
  //    surface it via the existing `handled && success === false`
  //    contract.
  if (retry?.isRetry) {
    try {
      assertIdempotentForRetry(retry.scriptName, record.capabilities);
    } catch (err) {
      return {
        ok: false,
        output: [],
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  const ctx: RunContext = {
    adapter: inner.adapter,
    transpile: inner.transpile,
    runtime: CLI_RUNTIME_DESCRIPTOR,
    invokedFrom,
    loopSymbol,
  };

  const result = await runScript(record, rawArgs, ctx);

  // 2. Capture-writeback per SPEC §7.4. Only applies when the chain
  //    step opted in via `captureTo: …` and the script returned
  //    something. Skip on `ok: false` — a failed script doesn't
  //    write its (possibly stale) returnValue.
  if (capture && result.ok && result.returnValue !== undefined) {
    const normalized = normalizeReturnForCapture(result.returnValue);
    if (normalized) {
      capture.preprocessStore.setVar(capture.captureTo, normalized.primary);
      if (normalized.json !== undefined) {
        capture.preprocessStore.setVar(`${capture.captureTo}.json`, normalized.json);
      }
    }
  }

  return result;
};
