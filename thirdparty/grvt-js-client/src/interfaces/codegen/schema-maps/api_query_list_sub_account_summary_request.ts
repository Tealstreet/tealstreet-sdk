import { type SchemaPairMap } from './types'

// Schema map for the 'API_QUERY_LIST_SUB_ACCOUNT_SUMMARY_REQUEST' struct.
export const API_QUERY_LIST_SUB_ACCOUNT_SUMMARY_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    time_interval: 'ti',
    sub_account_id: 'sa',
    start_time: 'st',
    end_time: 'et',
    sub_account_i_ds: 'sa1'
  },
  LITE_TO_FULL: {
    ti: 'time_interval',
    sa: 'sub_account_id',
    st: 'start_time',
    et: 'end_time',
    sa1: 'sub_account_i_ds'
  }
})
