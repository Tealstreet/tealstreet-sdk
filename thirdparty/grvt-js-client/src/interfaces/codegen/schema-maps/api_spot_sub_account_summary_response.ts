import { type SchemaPairMap } from './types'
import { SPOT_SUB_ACCOUNT_MAP } from './snap_spot_sub_account'

// Schema map for the 'API_SPOT_SUB_ACCOUNT_SUMMARY_RESPONSE' struct.
export const API_SPOT_SUB_ACCOUNT_SUMMARY_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', SPOT_SUB_ACCOUNT_MAP.FULL_TO_LITE]
  },
  LITE_TO_FULL: {
    r: ['result', SPOT_SUB_ACCOUNT_MAP.LITE_TO_FULL]
  }
})
