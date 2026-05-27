import { type SchemaPairMap } from './types'

// Schema map for the 'API_GET_SUB_ACCOUNT_POSITION_SUMMARY_REQUEST' struct.
export const API_GET_SUB_ACCOUNT_POSITION_SUMMARY_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_i_ds: 'sa',
    assets: 'a'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_i_ds',
    a: 'assets'
  }
})
