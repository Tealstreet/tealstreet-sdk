import { type SchemaPairMap } from './types'

// Schema map for the 'QUERY_SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_REQUEST' struct.
export const QUERY_SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_i_ds: 'sa',
    time_interval: 'ti',
    start_time: 'st',
    end_time: 'et',
    kind: 'k'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_i_ds',
    ti: 'time_interval',
    st: 'start_time',
    et: 'end_time',
    k: 'kind'
  }
})
