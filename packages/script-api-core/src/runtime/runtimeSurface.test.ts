import { describe, expect, it } from 'vitest';

import {
  CLI_READ_API_PATHS,
  getAllowedApiPathsForTargets,
  getApiPathAvailability,
  RUNTIME_SURFACES,
} from './runtimeSurface';

describe('runtime API surface descriptors', () => {
  it('keeps mobile read-only while covering portable market data reads', () => {
    expect(RUNTIME_SURFACES.mobile.apiPaths).toContain('api.getTicker');
    expect(RUNTIME_SURFACES.mobile.apiPaths).toContain('api.getOrderbook');
    expect(RUNTIME_SURFACES.mobile.apiPaths).toContain('api.getCandles');
    expect(RUNTIME_SURFACES.mobile.apiPaths).toContain('api.getHistoricalPosition');
    expect(RUNTIME_SURFACES.mobile.apiPaths).not.toContain('api.exchange.placeOrder');
    expect(RUNTIME_SURFACES.mobile.apiPaths).not.toContain('api.exchange.runCommand');
  });

  it('keeps CLI aligned to the portable read-only target surface', () => {
    expect(RUNTIME_SURFACES.cli.apiPaths).toContain('api.getCandles');
    expect(RUNTIME_SURFACES.cli.apiPaths).toContain('api.getOrderbook');
    expect(RUNTIME_SURFACES.cli.apiPaths).not.toContain('api.getHistoricalPosition');
    expect(RUNTIME_SURFACES.cli.apiPaths).not.toContain('api.exchange.placeOrder');
    expect(RUNTIME_SURFACES.cli.apiPaths).not.toContain('api.exchange.runCommand');
    expect(RUNTIME_SURFACES.cli.apiPaths).not.toContain('api.ui.getClick');
    expect(RUNTIME_SURFACES.cli.apiPaths).not.toContain('api.domHotkey.register');
  });

  it('computes target intersections without widening mobile', () => {
    const webMobile = getAllowedApiPathsForTargets(['core', 'web', 'mobile']);
    expect(webMobile.has('api.getTicker')).toBe(true);
    expect(webMobile.has('api.io.print')).toBe(true);
    expect(webMobile.has('api.exchange.placeOrder')).toBe(false);
    expect(webMobile.has('api.ui.getClick')).toBe(false);
  });

  it('uses all runnable targets as the portable core baseline when no runnable target is declared', () => {
    const portableCore = getAllowedApiPathsForTargets(['core']);
    for (const path of CLI_READ_API_PATHS) {
      expect(portableCore.has(path)).toBe(true);
    }
    expect(portableCore.has('api.getCandles')).toBe(true);
    expect(portableCore.has('api.getOrderbook')).toBe(true);
    expect(portableCore.has('api.getHistoricalPosition')).toBe(false);
    expect(portableCore.has('api.exchange.placeOrder')).toBe(false);
  });

  it('names the targets excluding a path and offers alternatives', () => {
    const write = getApiPathAvailability('api.exchange.placeOrder', ['core', 'web', 'mobile', 'cli']);
    expect(write.allowed).toBe(false);
    expect(write.excludedBy).toEqual(['web', 'mobile', 'cli']);
    expect(write.alternatives).toContain('api.getOrders');

    const webOnly = getApiPathAvailability('api.ui.getClick', ['core', 'web', 'mobile', 'cli']);
    expect(webOnly.allowed).toBe(false);
    expect(webOnly.excludedBy).toEqual(['mobile', 'cli']);
    expect(webOnly.alternatives).toContain('api.getTicker');

    const cliStubbed = getApiPathAvailability('api.getHistoricalPosition', ['core', 'web', 'mobile', 'cli']);
    expect(cliStubbed.allowed).toBe(false);
    expect(cliStubbed.excludedBy).toEqual(['cli']);
    expect(cliStubbed.alternatives).toContain('api.getPositions');
  });
});
