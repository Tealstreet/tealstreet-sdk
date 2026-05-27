import { type SchemaPairMap } from './types'
import { ACCOUNT_WITH_OVERRIDDEN_MULTIPLIER_INFO_MAP } from './account_with_overridden_multiplier_info'

// Schema map for the 'API_LIST_ACCOUNT_MULTIPLIER_WITH_OVERRIDE_RESPONSE' struct.
export const API_LIST_ACCOUNT_MULTIPLIER_WITH_OVERRIDE_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', [ACCOUNT_WITH_OVERRIDDEN_MULTIPLIER_INFO_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    r: ['result', [ACCOUNT_WITH_OVERRIDDEN_MULTIPLIER_INFO_MAP.LITE_TO_FULL]]
  }
})
