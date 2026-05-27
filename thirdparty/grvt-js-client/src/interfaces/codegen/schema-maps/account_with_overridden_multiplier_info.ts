import { type SchemaPairMap } from './types'
import { ACCOUNT_MULTIPLIER_METRIC_POINT_MAP } from './account_multiplier_metric_point'
import { ACCOUNT_MULTIPLIER_OVERRIDE_ENTRY_MAP } from './account_multiplier_override_entry'

// Schema map for the 'ACCOUNT_WITH_OVERRIDDEN_MULTIPLIER_INFO' struct.
export const ACCOUNT_WITH_OVERRIDDEN_MULTIPLIER_INFO_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    off_chain_account_id: 'oc',
    is_locked: 'il',
    total_point: 'tp',
    metric_points: ['mp', [ACCOUNT_MULTIPLIER_METRIC_POINT_MAP.FULL_TO_LITE]],
    overrides: ['o', [ACCOUNT_MULTIPLIER_OVERRIDE_ENTRY_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    oc: 'off_chain_account_id',
    il: 'is_locked',
    tp: 'total_point',
    mp: ['metric_points', [ACCOUNT_MULTIPLIER_METRIC_POINT_MAP.LITE_TO_FULL]],
    o: ['overrides', [ACCOUNT_MULTIPLIER_OVERRIDE_ENTRY_MAP.LITE_TO_FULL]]
  }
})
