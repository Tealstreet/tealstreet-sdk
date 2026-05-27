import { type SchemaPairMap } from './types'
import { SPOT_VIP_TIER_MAP } from './spot_vip_tier'

// Schema map for the 'SPOT_VIP_TIER_ASSET_CONFIG' struct.
export const SPOT_VIP_TIER_ASSET_CONFIG_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    currency_id: 'ci',
    tiers: ['t', [SPOT_VIP_TIER_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    ci: 'currency_id',
    t: ['tiers', [SPOT_VIP_TIER_MAP.LITE_TO_FULL]]
  }
})
