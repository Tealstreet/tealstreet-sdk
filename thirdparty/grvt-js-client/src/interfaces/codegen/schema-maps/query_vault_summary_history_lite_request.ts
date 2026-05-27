import { type SchemaPairMap } from './types'

// Schema map for the 'QUERY_VAULT_SUMMARY_HISTORY_LITE_REQUEST' struct.
export const QUERY_VAULT_SUMMARY_HISTORY_LITE_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    vault_id: 'vi',
    start_time: 'st',
    end_time: 'et',
    time_interval: 'ti',
    start_interval: 'si',
    limit: 'l',
    sort_by_event_time_desc: 'sb',
    include_vault_id: 'iv',
    include_total_lp_token_supply: 'it'
  },
  LITE_TO_FULL: {
    vi: 'vault_id',
    st: 'start_time',
    et: 'end_time',
    ti: 'time_interval',
    si: 'start_interval',
    l: 'limit',
    sb: 'sort_by_event_time_desc',
    iv: 'include_vault_id',
    it: 'include_total_lp_token_supply'
  }
})
