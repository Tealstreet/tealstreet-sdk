// Regression test for the TPSL_ORDER_METADATA_MAP patch (af66f1a7ae added
// is_split_position to both directions). The map drives serialisation of
// trigger / TP-SL orders; a missing field means the SDK will silently drop
// `is_split_position` from outgoing payloads, which GRVT then interprets
// as "whole-position" close and can cross positions the user didn't intend.
import { TPSL_ORDER_METADATA_MAP } from '../../grvt-js-client/src/interfaces/codegen/schema-maps/tpsl_order_metadata';

describe('TPSL_ORDER_METADATA_MAP (Tealstreet patch: is_split_position)', () => {
  it('FULL_TO_LITE contains is_split_position -> is', () => {
    expect((TPSL_ORDER_METADATA_MAP.FULL_TO_LITE as Record<string, unknown>).is_split_position).toBe('is');
  });

  it('LITE_TO_FULL contains is -> is_split_position', () => {
    expect((TPSL_ORDER_METADATA_MAP.LITE_TO_FULL as Record<string, unknown>).is).toBe('is_split_position');
  });
});
