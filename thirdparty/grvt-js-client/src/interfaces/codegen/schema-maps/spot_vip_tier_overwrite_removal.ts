import { type SchemaPairMap } from './types'

// Schema map for the 'SPOT_VIP_TIER_OVERWRITE_REMOVAL' struct.
export const SPOT_VIP_TIER_OVERWRITE_REMOVAL_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    main_account_id: 'ma',
    currency_id: 'ci'
  },
  LITE_TO_FULL: {
    ma: 'main_account_id',
    ci: 'currency_id'
  }
})
