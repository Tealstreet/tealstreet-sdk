/**
 * targetCheck — hard-fail-at-load surface + version validator. See SPEC §2.5.
 *
 * Pure function. Throws a precise Error with the offending target / version on
 * any mismatch; the caller (runScript, Phase 2) catches and surfaces to the UI.
 *
 * Runtime targets are a support set, not additive requirements. A script that
 * declares `['core', 'web', 'mobile', 'cli']` may run on any one of those
 * runtimes after authoring-time validation has constrained it to their API
 * intersection. This gate only checks the current host is included and that
 * the host-provided target versions are new enough.
 */
import type { TargetName } from '../api/capabilities.types';

export interface ScriptDescriptor {
  readonly targets: readonly TargetName[];
  /** Versions the script's editor stamped at save time. Missing == 0.0.0. */
  readonly coreApiVersion?: string;
  readonly cliApiVersion?: string;
  readonly webApiVersion?: string;
  readonly mobileApiVersion?: string;
}

export interface RuntimeDescriptor {
  /** Surfaces this runtime mounts. `core` is always present. */
  readonly mountedTargets: readonly TargetName[];
  /** Versions the runtime provides per surface. Missing means surface unsupported. */
  readonly coreApiVersion: string;
  readonly cliApiVersion?: string;
  readonly webApiVersion?: string;
  readonly mobileApiVersion?: string;
}

/**
 * Compare two semver-shaped version strings ("major.minor.patch"). Returns
 * positive if `a > b`, negative if `a < b`, 0 if equal. Treats missing
 * pieces as 0 so "1.2" compares as "1.2.0".
 */
const compareVersions = (a: string, b: string): number => {
  const pa = a.split('.').map((n) => Number.parseInt(n, 10) || 0);
  const pb = b.split('.').map((n) => Number.parseInt(n, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const ai = pa[i] ?? 0;
    const bi = pb[i] ?? 0;
    if (ai !== bi) return ai - bi;
  }
  return 0;
};

const VERSION_FIELD: Record<TargetName, keyof RuntimeDescriptor> = {
  core: 'coreApiVersion',
  cli: 'cliApiVersion',
  web: 'webApiVersion',
  mobile: 'mobileApiVersion',
};

const SCRIPT_VERSION_FIELD: Record<TargetName, keyof ScriptDescriptor> = {
  core: 'coreApiVersion',
  cli: 'cliApiVersion',
  web: 'webApiVersion',
  mobile: 'mobileApiVersion',
};

const SURFACE_LABEL: Record<TargetName, string> = {
  core: 'CORE',
  cli: 'CLI',
  web: 'WEB',
  mobile: 'MOBILE',
};

const TARGETS = new Set<TargetName>(['core', 'cli', 'web', 'mobile']);

/**
 * Validates a script's declared targets against the host runtime. Throws on
 * first failure with a script-author-facing message. Returns void on success.
 */
export const validateTargets = (script: ScriptDescriptor, runtime: RuntimeDescriptor): void => {
  const declaredTargets = script.targets.length > 0 ? script.targets : (['core'] as const);
  for (const target of declaredTargets) {
    if (!TARGETS.has(target)) {
      throw new Error(`unknown script target '${String(target)}'`);
    }
  }

  const runtimeRunnableTargets = runtime.mountedTargets.filter((target) => target !== 'core');
  const scriptRunnableTargets = declaredTargets.filter((target) => target !== 'core');
  const runnableMatches = scriptRunnableTargets.filter((target) => runtimeRunnableTargets.includes(target));
  if (scriptRunnableTargets.length > 0 && runnableMatches.length === 0) {
    throw new Error(
      `this script targets ${scriptRunnableTargets.map((target) => `'${target}'`).join(', ')}; this runtime supports ${runtimeRunnableTargets.map((target) => `'${target}'`).join(', ') || "'core-only'"}`,
    );
  }

  const targetsToVersionCheck = ['core', ...runnableMatches] as TargetName[];
  for (const target of targetsToVersionCheck) {
    if (!runtime.mountedTargets.includes(target)) {
      throw new Error(`this runtime does not mount the '${target}' API surface`);
    }
    const provided = runtime[VERSION_FIELD[target]];
    if (typeof provided !== 'string' || provided.length === 0) {
      throw new Error(`this script requires the '${target}' surface; runtime provides no '${target}' API version`);
    }
    const required = (script[SCRIPT_VERSION_FIELD[target]] as string | undefined) ?? '0.0.0';
    if (compareVersions(provided, required) < 0) {
      throw new Error(`script requires ${SURFACE_LABEL[target]} v${required}; runtime provides v${provided}`);
    }
  }
};
