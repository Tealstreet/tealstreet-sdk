import { type SchemaPairMap } from './types'

// Schema map for the 'API_POSITION_HISTORY_REQUEST' struct.
export const API_POSITION_HISTORY_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_id: 'sa',
    start_time: 'st',
    end_time: 'et',
    kind: 'k',
    base: 'b',
    quote: 'q',
    limit: 'l',
    cursor: 'c',
    status: 's',
    is_long: 'il',
    is_short: 'is',
    margin_type: 'mt'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_id',
    st: 'start_time',
    et: 'end_time',
    k: 'kind',
    b: 'base',
    q: 'quote',
    l: 'limit',
    c: 'cursor',
    s: 'status',
    il: 'is_long',
    is: 'is_short',
    mt: 'margin_type'
  }
})
