import { type SchemaPairMap } from './types'
import { SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_MAP } from './sub_account_trade_aggregation_by_interval_lite'

// Schema map for the 'QUERY_SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_RESPONSE' struct.
export const QUERY_SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', [SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    r: ['result', [SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_MAP.LITE_TO_FULL]]
  }
})
