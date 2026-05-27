import { type SchemaPairMap } from './types'

// Schema map for the 'FUNDING_ACCOUNT_POSITION_SUMMARY_RESULT' struct.
export const FUNDING_ACCOUNT_POSITION_SUMMARY_RESULT_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    main_account_id: 'ma',
    total_unrealized_pnl: 'tu',
    total_realized_pnl: 'tr'
  },
  LITE_TO_FULL: {
    ma: 'main_account_id',
    tu: 'total_unrealized_pnl',
    tr: 'total_realized_pnl'
  }
})
