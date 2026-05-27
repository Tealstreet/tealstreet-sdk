import { type SchemaPairMap } from './types'

// Schema map for the 'SUB_ACCOUNT_POSITION_SUMMARY_RESULT' struct.
export const SUB_ACCOUNT_POSITION_SUMMARY_RESULT_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_id: 'sa',
    total_unrealized_pnl: 'tu',
    total_realized_pnl: 'tr'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_id',
    tu: 'total_unrealized_pnl',
    tr: 'total_realized_pnl'
  }
})
