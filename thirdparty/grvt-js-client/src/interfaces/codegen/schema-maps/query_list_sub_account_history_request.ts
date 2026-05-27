import { type SchemaPairMap } from './types'

// Schema map for the 'QUERY_LIST_SUB_ACCOUNT_HISTORY_REQUEST' struct.
export const QUERY_LIST_SUB_ACCOUNT_HISTORY_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_i_ds: 'sa',
    start_time: 'st',
    end_time: 'et',
    limit: 'l',
    sort_by_time_desc: 'sb'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_i_ds',
    st: 'start_time',
    et: 'end_time',
    l: 'limit',
    sb: 'sort_by_time_desc'
  }
})
