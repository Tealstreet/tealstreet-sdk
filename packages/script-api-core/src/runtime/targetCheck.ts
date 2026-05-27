/**
 * targetCheck — hard-fail-at-load surface + version validator. See SPEC §2.5.
 *
 * Pure function. Throws a precise Error with the offending target / version on
 * any mismatch; the caller (runScript, Phase 2) catches and surfaces to the UI.
 *
 * Two failure modes:
 *   1. A declared `target` isn't mounted on this runtime → "this script
 *      requires the X surface; not available on …".
 *   2. The runtime's API version for a declared target is older than what
 *      the script requires → "script requires X v1.2.0; runtime provides v1.0.0".
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

/**
 * Validates a script's declared targets against the host runtime. Throws on
 * first failure with a script-author-facing message. Returns void on success.
 */
export const validateTargets = (script: ScriptDescriptor, runtime: RuntimeDescriptor): void => {
  for (const target of script.targets) {
    if (!runtime.mountedTargets.includes(target)) {
      throw new Error(`this script requires the '${target}' surface; not available on this runtime`);
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
