import { type SchemaPairMap } from './types'

// Schema map for the 'API_LOCK_ACCOUNT_MULTIPLIER_REQUEST' struct.
export const API_LOCK_ACCOUNT_MULTIPLIER_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    off_chain_account_id: 'oc',
    apply_to_direct_referrals: 'at'
  },
  LITE_TO_FULL: {
    oc: 'off_chain_account_id',
    at: 'apply_to_direct_referrals'
  }
})
