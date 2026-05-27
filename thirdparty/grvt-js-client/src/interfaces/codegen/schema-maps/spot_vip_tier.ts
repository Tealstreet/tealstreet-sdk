import { type SchemaPairMap } from './types'
import { SPOT_BORROW_CONFIG_MAP } from './spot_borrow_config'

// Schema map for the 'SPOT_VIP_TIER' struct.
export const SPOT_VIP_TIER_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    tier: 't',
    borrow_config: ['bc', SPOT_BORROW_CONFIG_MAP.FULL_TO_LITE]
  },
  LITE_TO_FULL: {
    t: 'tier',
    bc: ['borrow_config', SPOT_BORROW_CONFIG_MAP.LITE_TO_FULL]
  }
})
