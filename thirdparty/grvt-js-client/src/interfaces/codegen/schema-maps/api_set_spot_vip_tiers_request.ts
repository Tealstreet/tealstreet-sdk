import { type SchemaPairMap } from './types'
import { SPOT_VIP_TIER_MAP } from './spot_vip_tier'
import { SPOT_VIP_TIER_OVERWRITE_MAP } from './spot_vip_tier_overwrite'
import { SPOT_VIP_TIER_OVERWRITE_REMOVAL_MAP } from './spot_vip_tier_overwrite_removal'

// Schema map for the 'API_SET_SPOT_VIP_TIERS_REQUEST' struct.
export const API_SET_SPOT_VIP_TIERS_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    currency_id: 'ci',
    tiers: ['t', [SPOT_VIP_TIER_MAP.FULL_TO_LITE]],
    add_overwrites: ['ao', [SPOT_VIP_TIER_OVERWRITE_MAP.FULL_TO_LITE]],
    remove_overwrites: ['ro', [SPOT_VIP_TIER_OVERWRITE_REMOVAL_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    ci: 'currency_id',
    t: ['tiers', [SPOT_VIP_TIER_MAP.LITE_TO_FULL]],
    ao: ['add_overwrites', [SPOT_VIP_TIER_OVERWRITE_MAP.LITE_TO_FULL]],
    ro: ['remove_overwrites', [SPOT_VIP_TIER_OVERWRITE_REMOVAL_MAP.LITE_TO_FULL]]
  }
})
