import { type SchemaPairMap } from './types'
import { SUB_ACCOUNT_POSITION_SUMMARY_RESULT_MAP } from './sub_account_position_summary_result'

// Schema map for the 'API_GET_SUB_ACCOUNT_POSITION_SUMMARY_RESPONSE' struct.
export const API_GET_SUB_ACCOUNT_POSITION_SUMMARY_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', [SUB_ACCOUNT_POSITION_SUMMARY_RESULT_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    r: ['result', [SUB_ACCOUNT_POSITION_SUMMARY_RESULT_MAP.LITE_TO_FULL]]
  }
})
