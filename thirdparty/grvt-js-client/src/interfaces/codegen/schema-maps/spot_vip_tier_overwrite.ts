import { type SchemaPairMap } from './types'
import { SPOT_BORROW_CONFIG_MAP } from './spot_borrow_config'

// Schema map for the 'SPOT_VIP_TIER_OVERWRITE' struct.
export const SPOT_VIP_TIER_OVERWRITE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    main_account_id: 'ma',
    currency_id: 'ci',
    borrow_config: ['bc', SPOT_BORROW_CONFIG_MAP.FULL_TO_LITE]
  },
  LITE_TO_FULL: {
    ma: 'main_account_id',
    ci: 'currency_id',
    bc: ['borrow_config', SPOT_BORROW_CONFIG_MAP.LITE_TO_FULL]
  }
})
