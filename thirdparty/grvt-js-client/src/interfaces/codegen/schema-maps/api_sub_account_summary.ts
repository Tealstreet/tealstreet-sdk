import { type SchemaPairMap } from './types'

// Schema map for the 'API_SUB_ACCOUNT_SUMMARY' struct.
export const API_SUB_ACCOUNT_SUMMARY_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    event_time: 'et',
    total_equity: 'te',
    total_spot_balance: 'ts',
    total_future_equity: 'tf'
  },
  LITE_TO_FULL: {
    et: 'event_time',
    te: 'total_equity',
    ts: 'total_spot_balance',
    tf: 'total_future_equity'
  }
})
