import type { WooV3HttpMethod, WooV3MigrationGate, WooV3MigrationRoute } from './woo.v3.contracts';

import { WOO_V3_MIGRATION_GATES, WOO_V3_MIGRATION_ROUTES } from './woo.v3.contracts';

/**
 * Pure route selection and comparison primitives for a future WOO V3 runtime.
 *
 * This module owns no transport or store and is not imported by the active Woo
 * runtime. A shadow plan can observe only GET-to-GET migrations while the
 * legacy result remains authoritative. Writes are never shadowed because that
 * would dispatch the mutation twice, and V3 authority remains blocked by the
 * hard-false migration gates.
 */

export type WooV3MigrationRouteId = (typeof WOO_V3_MIGRATION_ROUTES)[number]['id'];
export type WooV3RuntimeMode = 'legacy' | 'shadow-v3' | 'v3';

export type WooV3RuntimeRouteTarget = Readonly<{
  generation: 'legacy' | 'v3';
  method: WooV3HttpMethod;
  path: string;
}>;

export type WooV3LegacyRouteSelection = Readonly<{
  status: 'ready';
  routeId: WooV3MigrationRouteId;
  requestedMode: 'legacy';
  gate: WooV3MigrationGate;
  authoritative: WooV3RuntimeRouteTarget & Readonly<{ generation: 'legacy' }>;
  storeAuthority: 'legacy-only';
}>;

export type WooV3ShadowReadSelection = Readonly<{
  status: 'ready';
  routeId: WooV3MigrationRouteId;
  requestedMode: 'shadow-v3';
  gate: 'publicReadParity' | 'privateReadParity';
  authoritative: WooV3RuntimeRouteTarget & Readonly<{ generation: 'legacy'; method: 'GET' }>;
  observer: WooV3RuntimeRouteTarget & Readonly<{ generation: 'v3'; method: 'GET' }>;
  storeAuthority: 'legacy-only';
  mutationDispatch: 'none';
}>;

export type WooV3V3RouteSelection = Readonly<{
  status: 'ready';
  routeId: WooV3MigrationRouteId;
  requestedMode: 'v3';
  gate: WooV3MigrationGate;
  authoritative: WooV3RuntimeRouteTarget & Readonly<{ generation: 'v3' }>;
  storeAuthority: 'v3-after-gate';
}>;

export type WooV3AlreadyV3RouteSelection = Readonly<{
  status: 'ready';
  routeId: WooV3MigrationRouteId;
  requestedMode: 'legacy' | 'v3';
  gate: WooV3MigrationGate;
  authoritative: WooV3RuntimeRouteTarget & Readonly<{ generation: 'v3'; method: 'GET' }>;
  storeAuthority: 'existing-v3';
}>;

export type WooV3BlockedRouteSelection = Readonly<{
  status: 'blocked';
  routeId: WooV3MigrationRouteId;
  requestedMode: WooV3RuntimeMode;
  gate: WooV3MigrationGate;
  blocker:
    | 'migration-gate-closed'
    | 'shadow-mode-is-read-only'
    | 'cancel-all-after-has-no-runtime-owner'
    | 'route-has-no-runtime-owner'
    | 'route-is-already-v3';
  storeAuthority: 'legacy-only';
}>;

export type WooV3InvalidModeSelection = Readonly<{
  status: 'blocked';
  routeId: WooV3MigrationRouteId;
  requestedMode: unknown;
  gate: WooV3MigrationGate;
  blocker: 'invalid-runtime-mode';
  storeAuthority: 'legacy-only';
}>;

export type WooV3RuntimeRouteSelection =
  | WooV3LegacyRouteSelection
  | WooV3ShadowReadSelection
  | WooV3V3RouteSelection
  | WooV3AlreadyV3RouteSelection
  | WooV3BlockedRouteSelection
  | WooV3InvalidModeSelection;

const runtimeGates: Readonly<Record<WooV3MigrationGate, boolean>> = WOO_V3_MIGRATION_GATES;

const routeById = new Map<WooV3MigrationRouteId, WooV3MigrationRoute>(
  WOO_V3_MIGRATION_ROUTES.map((route) => [route.id, route]),
);

const requireRoute = (routeId: WooV3MigrationRouteId): WooV3MigrationRoute => {
  const route = routeById.get(routeId);
  if (!route) throw new Error(`Unknown WOO V3 migration route ${String(routeId)}`);
  return route;
};

const target = (
  generation: WooV3RuntimeRouteTarget['generation'],
  method: WooV3HttpMethod,
  path: string,
): WooV3RuntimeRouteTarget => Object.freeze({ generation, method, path });

/**
 * Resolves a transport-neutral route plan. It cannot send a request or mutate
 * state, and it never accepts caller-supplied gate overrides.
 */
export const selectWooV3RuntimeRoute = (
  routeId: WooV3MigrationRouteId,
  requestedMode: WooV3RuntimeMode,
): WooV3RuntimeRouteSelection => {
  const route = requireRoute(routeId);

  if (requestedMode !== 'legacy' && requestedMode !== 'shadow-v3' && requestedMode !== 'v3') {
    return Object.freeze({
      status: 'blocked',
      routeId,
      requestedMode,
      gate: route.gate,
      blocker: 'invalid-runtime-mode',
      storeAuthority: 'legacy-only',
    });
  }

  const runtimeLifecycle = 'runtimeLifecycle' in route ? route.runtimeLifecycle : undefined;

  if (runtimeLifecycle === 'ownerless') {
    return Object.freeze({
      status: 'blocked',
      routeId,
      requestedMode,
      gate: route.gate,
      blocker: route.id === 'cancel-all-after' ? 'cancel-all-after-has-no-runtime-owner' : 'route-has-no-runtime-owner',
      storeAuthority: 'legacy-only',
    });
  }

  if (runtimeLifecycle === 'already-v3') {
    if (requestedMode === 'shadow-v3') {
      return Object.freeze({
        status: 'blocked',
        routeId,
        requestedMode,
        gate: route.gate,
        blocker: 'route-is-already-v3',
        storeAuthority: 'legacy-only',
      });
    }
    return Object.freeze({
      status: 'ready',
      routeId,
      requestedMode,
      gate: route.gate,
      authoritative: target('v3', 'GET', route.v3Path) as WooV3AlreadyV3RouteSelection['authoritative'],
      storeAuthority: 'existing-v3',
    });
  }

  if (requestedMode === 'legacy') {
    return Object.freeze({
      status: 'ready',
      routeId,
      requestedMode,
      gate: route.gate,
      authoritative: target(
        'legacy',
        route.legacyMethod,
        route.legacyPath,
      ) as WooV3LegacyRouteSelection['authoritative'],
      storeAuthority: 'legacy-only',
    });
  }

  if (requestedMode === 'shadow-v3') {
    if (route.legacyMethod !== 'GET' || route.v3Method !== 'GET') {
      return Object.freeze({
        status: 'blocked',
        routeId,
        requestedMode,
        gate: route.gate,
        blocker: 'shadow-mode-is-read-only',
        storeAuthority: 'legacy-only',
      });
    }
    if (route.gate !== 'publicReadParity' && route.gate !== 'privateReadParity') {
      throw new Error(`WOO V3 read route ${route.id} must use a read-parity gate`);
    }
    return Object.freeze({
      status: 'ready',
      routeId,
      requestedMode,
      gate: route.gate,
      authoritative: target('legacy', 'GET', route.legacyPath) as WooV3ShadowReadSelection['authoritative'],
      observer: target('v3', 'GET', route.v3Path) as WooV3ShadowReadSelection['observer'],
      storeAuthority: 'legacy-only',
      mutationDispatch: 'none',
    });
  }

  if (!runtimeGates[route.gate]) {
    return Object.freeze({
      status: 'blocked',
      routeId,
      requestedMode,
      gate: route.gate,
      blocker: 'migration-gate-closed',
      storeAuthority: 'legacy-only',
    });
  }

  return Object.freeze({
    status: 'ready',
    routeId,
    requestedMode,
    gate: route.gate,
    authoritative: target('v3', route.v3Method, route.v3Path) as WooV3V3RouteSelection['authoritative'],
    storeAuthority: 'v3-after-gate',
  });
};

export type WooV3ObservedCanonical<TObserved> = Readonly<{
  canonical: string;
  observed: TObserved;
}>;

export type WooV3ShadowReadComparison<TLegacy, TObserved> =
  | Readonly<{
      status: 'match' | 'mismatch';
      routeId: WooV3MigrationRouteId;
      authoritative: TLegacy;
      observedCanonical: string;
      observed: TObserved;
      storeAction: 'retain-legacy';
    }>
  | Readonly<{
      status: 'decode-or-map-failed';
      routeId: WooV3MigrationRouteId;
      authoritative: TLegacy;
      error: Error;
      storeAction: 'retain-legacy';
    }>;

const normalizeError = (error: unknown): Error =>
  error instanceof Error ? error : new Error(`Unknown WOO V3 shadow error: ${String(error)}`);

/**
 * Runs a synchronous exact comparison without exposing the authoritative
 * legacy snapshot to a callback. The caller supplies an already-built legacy
 * canonical string, while the only callback decodes the V3 response and
 * returns both its endpoint canonical string and observed value. The observed
 * value remains available as provenance; the legacy snapshot is returned by
 * identity in every outcome and remains the only store authority.
 */
export const compareWooV3ShadowRead = <TLegacySnapshot, TObserved>(
  input: Readonly<{
    selection: WooV3ShadowReadSelection;
    legacySnapshot: TLegacySnapshot;
    legacyCanonical: string;
    v3Response: unknown;
    decodeAndCanonicalizeV3: (response: unknown) => WooV3ObservedCanonical<TObserved>;
  }>,
): WooV3ShadowReadComparison<TLegacySnapshot, TObserved> => {
  let observedCanonical: string;
  let observed: TObserved;
  try {
    const decoded = input.decodeAndCanonicalizeV3(input.v3Response);
    if (
      typeof decoded !== 'object' ||
      decoded === null ||
      !Object.prototype.hasOwnProperty.call(decoded, 'canonical') ||
      !Object.prototype.hasOwnProperty.call(decoded, 'observed')
    ) {
      throw new Error('WOO V3 shadow decoder must return a canonical string and its observed value');
    }
    observedCanonical = decoded.canonical;
    observed = decoded.observed;
    if (typeof observedCanonical !== 'string') {
      throw new Error('WOO V3 shadow decoder must return a canonical string and its observed value');
    }
  } catch (error) {
    return Object.freeze({
      status: 'decode-or-map-failed',
      routeId: input.selection.routeId,
      authoritative: input.legacySnapshot,
      error: normalizeError(error),
      storeAction: 'retain-legacy',
    });
  }

  return Object.freeze({
    status: input.legacyCanonical === observedCanonical ? 'match' : 'mismatch',
    routeId: input.selection.routeId,
    authoritative: input.legacySnapshot,
    observedCanonical,
    observed,
    storeAction: 'retain-legacy',
  });
};
