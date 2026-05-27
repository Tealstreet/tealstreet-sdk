import { type SchemaPairMap } from './types'
import { API_SUB_ACCOUNT_TRADE_AGGREGATION_TREND_MAP } from './api_sub_account_trade_aggregation_trend'

// Schema map for the 'API_QUERY_SUB_ACCOUNT_TRADE_AGGREGATION_TREND_RESPONSE' struct.
export const API_QUERY_SUB_ACCOUNT_TRADE_AGGREGATION_TREND_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', [API_SUB_ACCOUNT_TRADE_AGGREGATION_TREND_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    r: ['result', [API_SUB_ACCOUNT_TRADE_AGGREGATION_TREND_MAP.LITE_TO_FULL]]
  }
})
