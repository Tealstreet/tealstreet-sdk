import * as fs from 'fs';
import * as path from 'path';

import { describe, expect, it } from 'vitest';

import { generateCoreReadManifest, renderManifestFile } from '../../scripts/generate-manifest';
import { CORE_READ_MANIFEST } from './read-manifest';
import { CORE_READ_API_PATHS } from './runtimeSurface';

/**
 * Drift gate for the generated read manifest.
 *
 * Unlike `script-types-bundle.test.ts`, this one needs no `describe.skipIf`
 * machinery: generator, artifact and gate all live inside this package, which
 * `mirrors/copy.bara.sky` carries into the OSS mirror whole — so a static
 * import resolves in both trees and the gate runs in both.
 */
const ON_DISK_PATH = path.join(__dirname, 'read-manifest.ts');

describe('read-manifest drift', () => {
  it('on-disk artifact matches what the generator emits right now', () => {
    const onDisk = fs.readFileSync(ON_DISK_PATH, 'utf8');
    expect(renderManifestFile(generateCoreReadManifest())).toBe(onDisk);
  });
});

describe('the manifest covers the whole read slice', () => {
  it('names exactly the paths in CORE_READ_API_PATHS', () => {
    expect(CORE_READ_MANIFEST.map((entry) => entry.path).sort()).toEqual([...CORE_READ_API_PATHS].sort());
  });
});

/**
 * The artifact is consumed by shells that hand it to a model. An entry with no
 * summary is a tool with no description, which measurably degrades tool choice.
 */
describe('every manifest entry is describable', () => {
  it('carries a non-empty summary and a known cost class', () => {
    for (const entry of CORE_READ_MANIFEST) {
      expect(entry.summary.length).toBeGreaterThan(10);
      expect(['store', 'network']).toContain(entry.costClass);
    }
  });

  /**
   * `resultShape` is what a shell bounds a result BY. An entry without one is
   * a read whose cap has to be guessed from the value, which is the state this
   * field was added to end — `boundReadResult` used to string-match two method
   * names against a parameter typed `string`, so a rename dropped the cap in
   * silence.
   */
  it('declares how its result is shaped, so a shell need not guess', () => {
    for (const entry of CORE_READ_MANIFEST) {
      expect(['candles', 'orderbook', 'rows', 'scalar'], entry.method).toContain(entry.resultShape);
    }
  });

  it('keeps exactly one candle read and one book read', () => {
    const by = (shape: string) => CORE_READ_MANIFEST.filter((entry) => entry.resultShape === shape).map((e) => e.method);
    expect(by('candles')).toEqual(['getCandles']);
    expect(by('orderbook')).toEqual(['getOrderbook']);
  });

  it('describes every parameter it declares', () => {
    for (const entry of CORE_READ_MANIFEST) {
      for (const [name, field] of Object.entries(entry.params.properties)) {
        expect(field.description, `${entry.method}.${name}`).toBeTruthy();
      }
    }
  });
});
