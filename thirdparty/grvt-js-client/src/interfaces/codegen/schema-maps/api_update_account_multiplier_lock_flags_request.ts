import { type SchemaPairMap } from './types'

// Schema map for the 'API_UPDATE_ACCOUNT_MULTIPLIER_LOCK_FLAGS_REQUEST' struct.
export const API_UPDATE_ACCOUNT_MULTIPLIER_LOCK_FLAGS_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    off_chain_account_id: 'oc',
    apply_to_direct_referrals: 'at'
  },
  LITE_TO_FULL: {
    oc: 'off_chain_account_id',
    at: 'apply_to_direct_referrals'
  }
})
