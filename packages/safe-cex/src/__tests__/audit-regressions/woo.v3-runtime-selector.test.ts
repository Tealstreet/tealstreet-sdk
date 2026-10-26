import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import { WOO_V3_MIGRATION_GATES, WOO_V3_MIGRATION_ROUTES } from '../reference/woo/woo.v3.contracts';
import { compareWooV3ShadowRead, selectWooV3RuntimeRoute } from '../reference/woo/woo.v3.runtime-selector';

describe('WOO V3 inert runtime route selector', () => {
  it('keeps legacy authoritative by default and exposes exact route provenance', () => {
    expect(selectWooV3RuntimeRoute('markets', 'legacy')).toEqual({
      status: 'ready',
      routeId: 'markets',
      requestedMode: 'legacy',
      gate: 'publicReadParity',
      authoritative: {
        generation: 'legacy',
        method: 'GET',
        path: '/v1/public/info',
      },
      storeAuthority: 'legacy-only',
    });

    expect(selectWooV3RuntimeRoute('place-order', 'legacy')).toMatchObject({
      status: 'ready',
      authoritative: { generation: 'legacy', method: 'POST', path: '/v1/order' },
      storeAuthority: 'legacy-only',
    });
  });

  it('builds shadow plans only for GET-to-GET migrations with no V3 store authority', () => {
    const readRoutes = WOO_V3_MIGRATION_ROUTES.filter(
      (route) => route.legacyMethod === 'GET' && route.v3Method === 'GET' && !('runtimeLifecycle' in route),
    );

    for (const route of readRoutes) {
      expect(selectWooV3RuntimeRoute(route.id, 'shadow-v3')).toEqual({
        status: 'ready',
        routeId: route.id,
        requestedMode: 'shadow-v3',
        gate: route.gate,
        authoritative: {
          generation: 'legacy',
          method: 'GET',
          path: route.legacyPath,
        },
        observer: {
          generation: 'v3',
          method: 'GET',
          path: route.v3Path,
        },
        storeAuthority: 'legacy-only',
        mutationDispatch: 'none',
      });
    }
  });

  it('blocks every write from shadow mode instead of duplicating a mutation', () => {
    const writeRoutes = WOO_V3_MIGRATION_ROUTES.filter(
      (route) => !('runtimeLifecycle' in route) && (route.legacyMethod !== 'GET' || route.v3Method !== 'GET'),
    );

    for (const route of writeRoutes) {
      expect(selectWooV3RuntimeRoute(route.id, 'shadow-v3')).toEqual({
        status: 'blocked',
        routeId: route.id,
        requestedMode: 'shadow-v3',
        gate: route.gate,
        blocker: 'shadow-mode-is-read-only',
        storeAuthority: 'legacy-only',
      });
    }
  });

  it('blocks V3 authority on every owned migration while its hard gate is false', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);

    for (const route of WOO_V3_MIGRATION_ROUTES.filter((candidate) => !('runtimeLifecycle' in candidate))) {
      const selection = selectWooV3RuntimeRoute(route.id, 'v3');
      expect(selection).toMatchObject({
        status: 'blocked',
        routeId: route.id,
        requestedMode: 'v3',
        storeAuthority: 'legacy-only',
      });
      expect(selection.status === 'blocked' ? selection.blocker : undefined).toBe('migration-gate-closed');
    }
  });

  it('blocks every mode for routes with no active runtime owner', () => {
    for (const routeId of ['balances', 'cancel-all-algo-orders'] as const) {
      for (const requestedMode of ['legacy', 'shadow-v3', 'v3'] as const) {
        expect(selectWooV3RuntimeRoute(routeId, requestedMode)).toEqual({
          status: 'blocked',
          routeId,
          requestedMode,
          gate: routeId === 'balances' ? 'privateReadParity' : 'algoWriteDemoLifecycle',
          blocker: 'route-has-no-runtime-owner',
          storeAuthority: 'legacy-only',
        });
      }
    }

    for (const requestedMode of ['legacy', 'shadow-v3', 'v3'] as const) {
      expect(selectWooV3RuntimeRoute('cancel-all-after', requestedMode)).toMatchObject({
        status: 'blocked',
        blocker: 'cancel-all-after-has-no-runtime-owner',
      });
    }
  });

  it('treats the active transaction-history V3 route as a no-op instead of shadowing it', () => {
    for (const requestedMode of ['legacy', 'v3'] as const) {
      expect(selectWooV3RuntimeRoute('transaction-history', requestedMode)).toEqual({
        status: 'ready',
        routeId: 'transaction-history',
        requestedMode,
        gate: 'privateReadParity',
        authoritative: {
          generation: 'v3',
          method: 'GET',
          path: '/v3/trade/transactionHistory',
        },
        storeAuthority: 'existing-v3',
      });
    }

    expect(selectWooV3RuntimeRoute('transaction-history', 'shadow-v3')).toEqual({
      status: 'blocked',
      routeId: 'transaction-history',
      requestedMode: 'shadow-v3',
      gate: 'privateReadParity',
      blocker: 'route-is-already-v3',
      storeAuthority: 'legacy-only',
    });
  });

  it('fails closed and preserves an unknown runtime mode instead of falling through to V3', () => {
    const requestedMode = 'v33' as never;
    expect(selectWooV3RuntimeRoute('markets', requestedMode)).toEqual({
      status: 'blocked',
      routeId: 'markets',
      requestedMode: 'v33',
      gate: 'publicReadParity',
      blocker: 'invalid-runtime-mode',
      storeAuthority: 'legacy-only',
    });
  });

  it('retains the legacy snapshot by identity on exact canonical match, mismatch, and decode failure', () => {
    const selection = selectWooV3RuntimeRoute('markets', 'shadow-v3');
    if (selection.status !== 'ready' || selection.requestedMode !== 'shadow-v3') {
      throw new Error('expected a shadow read selection');
    }
    const legacySnapshot = Object.freeze([{ symbol: 'BTCUSDT' }]);
    const observed = Object.freeze([{ symbol: 'BTCUSDT' }]);
    const decodeAndCanonicalizeV3 = vi.fn(() =>
      Object.freeze({ canonical: JSON.stringify(observed), observed }),
    );

    const match = compareWooV3ShadowRead({
      selection,
      legacySnapshot,
      legacyCanonical: JSON.stringify(legacySnapshot),
      v3Response: { fixture: true },
      decodeAndCanonicalizeV3,
    });
    expect(match).toMatchObject({
      status: 'match',
      observedCanonical: '[{"symbol":"BTCUSDT"}]',
      storeAction: 'retain-legacy',
    });
    expect(match.authoritative).toBe(legacySnapshot);
    expect(match.status === 'match' && match.observed).toBe(observed);
    expect(decodeAndCanonicalizeV3).toHaveBeenCalledWith({ fixture: true });

    const mismatch = compareWooV3ShadowRead({
      selection,
      legacySnapshot,
      legacyCanonical: JSON.stringify(legacySnapshot),
      v3Response: observed,
      decodeAndCanonicalizeV3: () => ({ canonical: '[{"symbol":"ETHUSDT"}]', observed }),
    });
    expect(mismatch).toMatchObject({ status: 'mismatch', storeAction: 'retain-legacy' });
    expect(mismatch.authoritative).toBe(legacySnapshot);

    const decodeFailure = compareWooV3ShadowRead({
      selection,
      legacySnapshot,
      legacyCanonical: JSON.stringify(legacySnapshot),
      v3Response: null,
      decodeAndCanonicalizeV3: () => {
        throw new Error('malformed V3 envelope');
      },
    });
    expect(decodeFailure).toMatchObject({
      status: 'decode-or-map-failed',
      error: new Error('malformed V3 envelope'),
      storeAction: 'retain-legacy',
    });
    expect(decodeFailure.authoritative).toBe(legacySnapshot);
  });

  it('uses a primitive canonical boundary that cannot carry nested aliases into callbacks', () => {
    const selection = selectWooV3RuntimeRoute('markets', 'shadow-v3');
    if (selection.status !== 'ready' || selection.requestedMode !== 'shadow-v3') {
      throw new Error('expected a shadow read selection');
    }
    const legacySnapshot = { rows: [{ symbol: 'BTCUSDT', active: true }] };
    const nestedAlias = { rows: legacySnapshot.rows };
    const legacyCanonical = JSON.stringify(nestedAlias);

    const comparison = compareWooV3ShadowRead({
      selection,
      legacySnapshot,
      legacyCanonical,
      v3Response: [{ symbol: 'BTCUSDT', active: false }],
      decodeAndCanonicalizeV3: (value) => {
        const parsedCanonical = JSON.parse(legacyCanonical) as typeof nestedAlias;
        parsedCanonical.rows.length = 0;
        return { canonical: JSON.stringify({ rows: value }), observed: value };
      },
    });

    expect(comparison).toMatchObject({ status: 'mismatch', storeAction: 'retain-legacy' });
    expect(comparison.authoritative).toBe(legacySnapshot);
    expect(legacySnapshot).toEqual({ rows: [{ symbol: 'BTCUSDT', active: true }] });
    expect(nestedAlias.rows).toBe(legacySnapshot.rows);
  });

  it('fails closed when the V3 decoder omits or cannot expose its observed value', () => {
    const selection = selectWooV3RuntimeRoute('markets', 'shadow-v3');
    if (selection.status !== 'ready' || selection.requestedMode !== 'shadow-v3') {
      throw new Error('expected a shadow read selection');
    }
    const legacySnapshot = [{ symbol: 'BTCUSDT' }];
    const legacyCanonical = JSON.stringify(legacySnapshot);

    const missingObserved = compareWooV3ShadowRead({
      selection,
      legacySnapshot,
      legacyCanonical,
      v3Response: {},
      decodeAndCanonicalizeV3: () => ({ canonical: legacyCanonical }) as never,
    });
    expect(missingObserved).toMatchObject({
      status: 'decode-or-map-failed',
      error: expect.objectContaining({ message: expect.stringContaining('canonical string and its observed value') }),
      storeAction: 'retain-legacy',
    });

    const throwingObserved = compareWooV3ShadowRead({
      selection,
      legacySnapshot,
      legacyCanonical,
      v3Response: {},
      decodeAndCanonicalizeV3: () =>
        ({
          canonical: legacyCanonical,
          get observed() {
            throw new Error('observed getter exploded');
          },
        }) as never,
    });
    expect(throwingObserved).toMatchObject({
      status: 'decode-or-map-failed',
      error: new Error('observed getter exploded'),
      storeAction: 'retain-legacy',
    });
  });

  it('remains disconnected from active Woo runtime and owns no transport or store', () => {
    const wooDirectory = join(__dirname, '../reference/woo');
    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-public.ts', 'woo.ws-private.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.runtime-selector');
    }

    const source = readFileSync(join(wooDirectory, 'woo.v3.runtime-selector.ts'), 'utf8');
    expect(source).not.toMatch(/from ['"](?:axios|ws|@tealstreet\/reconnecting-websocket)['"]/);
    expect(source).not.toMatch(/\bnew WebSocket\b|\bfetch\s*\(|\.request\s*\(|\.store\b/);
  });
});
