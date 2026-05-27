import { type SchemaPairMap } from './types'
import { SPOT_BALANCE_MAP } from './api_spot_balance'

// Schema map for the 'SPOT_SUB_ACCOUNT' struct.
export const SPOT_SUB_ACCOUNT_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    event_time: 'et',
    sub_account_id: 'sa',
    total_equity: 'te',
    spot_balances: ['sb', [SPOT_BALANCE_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    et: 'event_time',
    sa: 'sub_account_id',
    te: 'total_equity',
    sb: ['spot_balances', [SPOT_BALANCE_MAP.LITE_TO_FULL]]
  }
})
