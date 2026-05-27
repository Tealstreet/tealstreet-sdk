// Regression tests for the grvt-js-client ORDER_MAP patches:
//   - b1125ef417 fix(grvt): add builder/builder_fee to ORDER_MAP so SDK doesn't drop them
//   - af66f1a7ae fix(grvt): move builder/builder_fee after signature and metadata
//
// The SDK's schemaMap walker preserves JS object-key insertion order when
// serialising. Our GRVT patches require:
//   1. builder (b) and builder_fee (bf) are present in both FULL_TO_LITE
//      and LITE_TO_FULL maps, so they survive the lite<->full round-trip.
//   2. In FULL_TO_LITE, builder/builder_fee appear *after* signature ('s')
//      and metadata ('m'), so the emitted API payload has them at the tail
//      (GRVT's API expects that order).
//
// These tests lock the patches in place before an upstream sync — any
// upstream change that removes them or reorders them will fail here.
import { ORDER_MAP } from '../../grvt-js-client/src/interfaces/codegen/schema-maps/order';

describe('ORDER_MAP (Tealstreet patch: builder / builder_fee)', () => {
  describe('FULL_TO_LITE', () => {
    it('contains builder and builder_fee', () => {
      const full = ORDER_MAP.FULL_TO_LITE as Record<string, unknown>;
      expect(full.builder).toBe('b');
      expect(full.builder_fee).toBe('bf');
    });

    it('places builder and builder_fee after signature + metadata', () => {
      const keys = Object.keys(ORDER_MAP.FULL_TO_LITE);
      const signatureIdx = keys.indexOf('signature');
      const metadataIdx = keys.indexOf('metadata');
      const builderIdx = keys.indexOf('builder');
      const builderFeeIdx = keys.indexOf('builder_fee');
      expect(signatureIdx).toBeGreaterThanOrEqual(0);
      expect(metadataIdx).toBeGreaterThanOrEqual(0);
      expect(builderIdx).toBeGreaterThan(signatureIdx);
      expect(builderIdx).toBeGreaterThan(metadataIdx);
      expect(builderFeeIdx).toBeGreaterThan(signatureIdx);
      expect(builderFeeIdx).toBeGreaterThan(metadataIdx);
    });
  });

  describe('LITE_TO_FULL', () => {
    it('contains the b and bf inverse mappings', () => {
      const lite = ORDER_MAP.LITE_TO_FULL as Record<string, unknown>;
      expect(lite.b).toBe('builder');
      expect(lite.bf).toBe('builder_fee');
    });
  });
});
