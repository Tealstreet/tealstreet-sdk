import { type SchemaPairMap } from './types'

// Schema map for the 'API_GET_SUB_ACCOUNTS_RESPONSE' struct.
export const API_GET_SUB_ACCOUNTS_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_ids: 'sa'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_ids'
  }
})
