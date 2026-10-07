import { describe, expect, it } from 'vitest';

import {
  AGENT_READ_API_PATHS,
  CORE_READ_API_PATHS,
  CORE_WRITE_API_PATHS,
} from '@tealstreet/script-api-core/src/runtime/runtimeSurface';

import { isOemsReadMethod, isOemsWriteMethod, OEMS_READ_METHODS, OEMS_WRITE_METHODS } from './surface';

/**
 * The slice is only meaningful while it is a SUBSET of the surface the shared
 * SDK actually serves. A method that survives here after being dropped from
 * `CORE_*_API_PATHS` describes an API nobody implements, and the failure would
 * land at the point of trade rather than at the point of drift.
 */

const methodsOf = (paths: readonly string[]): ReadonlySet<string> =>
  new Set(paths.map((path) => path.slice(path.lastIndexOf('.') + 1)));

describe('OEMS surface', () => {
  it('every write method is carried by the SDK write surface', () => {
    const carried = methodsOf(CORE_WRITE_API_PATHS);
    for (const method of OEMS_WRITE_METHODS) expect(carried.has(method)).toBe(true);
  });

  it('every read method is carried by the SDK read surface', () => {
    const carried = methodsOf(CORE_READ_API_PATHS);
    for (const method of OEMS_READ_METHODS) expect(carried.has(method)).toBe(true);
  });

  it('is a strict narrowing, not a copy', () => {
    expect(OEMS_WRITE_METHODS.length).toBeLessThan(CORE_WRITE_API_PATHS.length);
    expect(OEMS_READ_METHODS.length).toBeLessThan(CORE_READ_API_PATHS.length);
  });

  it('carries no duplicates and is frozen', () => {
    expect(new Set(OEMS_WRITE_METHODS).size).toBe(OEMS_WRITE_METHODS.length);
    expect(new Set(OEMS_READ_METHODS).size).toBe(OEMS_READ_METHODS.length);
    expect(Object.isFrozen(OEMS_WRITE_METHODS)).toBe(true);
    expect(Object.isFrozen(OEMS_READ_METHODS)).toBe(true);
  });

  it('excludes account configuration and the REPL escape hatch', () => {
    const carried = methodsOf(CORE_WRITE_API_PATHS);
    for (const method of ['setPositionMode', 'setMarginType', 'runCommand']) {
      expect(isOemsWriteMethod(method)).toBe(false);
      expect(carried.has(method)).toBe(true);
    }
  });

  it('excludes the analysis-input reads', () => {
    const carried = methodsOf(CORE_READ_API_PATHS);
    for (const method of ['getCandles', 'getOrderbook', 'getHistoricalPosition']) {
      expect(isOemsReadMethod(method)).toBe(false);
      expect(carried.has(method)).toBe(true);
    }
  });

  it('guards reject unknown values', () => {
    expect(isOemsWriteMethod('nuke')).toBe(false);
    expect(isOemsWriteMethod('placeMarketOrder')).toBe(false);
    expect(isOemsWriteMethod(undefined)).toBe(false);
    expect(isOemsReadMethod('getCandles')).toBe(false);
    expect(isOemsReadMethod(7)).toBe(false);
  });
});

/**
 * The agent read slice is EQUAL to core, and the tests above assert STRICT
 * NARROWING for the OEMS read slice. Both are deliberate and they pull in
 * opposite directions, so this is asserted explicitly rather than left to
 * whoever next reads the narrowing helper and assumes every slice narrows.
 *
 * The agent is bounded on WRITES plus a human confirmation. Narrowing its
 * reads would remove the analysis inputs the chained-read work exists to add.
 */
describe('AGENT_READ_API_PATHS', () => {
  it('equals the core read surface — narrowing it would be a regression', () => {
    expect([...AGENT_READ_API_PATHS].sort()).toEqual([...CORE_READ_API_PATHS].sort());
  });
});
