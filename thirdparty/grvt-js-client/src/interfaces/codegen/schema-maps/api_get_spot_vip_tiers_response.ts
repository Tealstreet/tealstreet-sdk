import { type SchemaPairMap } from './types'
import { SPOT_VIP_TIER_ASSET_CONFIG_MAP } from './spot_vip_tier_asset_config'
import { SPOT_VIP_TIER_OVERWRITE_MAP } from './spot_vip_tier_overwrite'

// Schema map for the 'API_GET_SPOT_VIP_TIERS_RESPONSE' struct.
export const API_GET_SPOT_VIP_TIERS_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    assets: ['a', [SPOT_VIP_TIER_ASSET_CONFIG_MAP.FULL_TO_LITE]],
    overwrites: ['o', [SPOT_VIP_TIER_OVERWRITE_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    a: ['assets', [SPOT_VIP_TIER_ASSET_CONFIG_MAP.LITE_TO_FULL]],
    o: ['overwrites', [SPOT_VIP_TIER_OVERWRITE_MAP.LITE_TO_FULL]]
  }
})
