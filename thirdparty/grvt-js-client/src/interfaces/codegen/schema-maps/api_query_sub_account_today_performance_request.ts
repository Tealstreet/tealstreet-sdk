import { type SchemaPairMap } from './types'

// Schema map for the 'API_QUERY_SUB_ACCOUNT_TODAY_PERFORMANCE_REQUEST' struct.
export const API_QUERY_SUB_ACCOUNT_TODAY_PERFORMANCE_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_id: 'sa',
    currency: 'c',
    main_account_id: 'ma'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_id',
    c: 'currency',
    ma: 'main_account_id'
  }
})
