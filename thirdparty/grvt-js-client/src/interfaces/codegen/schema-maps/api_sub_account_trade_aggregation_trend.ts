import { type SchemaPairMap } from './types'

// Schema map for the 'API_SUB_ACCOUNT_TRADE_AGGREGATION_TREND' struct.
export const API_SUB_ACCOUNT_TRADE_AGGREGATION_TREND_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    start_interval: 'si',
    end_interval: 'ei',
    trading_volume: 'tv'
  },
  LITE_TO_FULL: {
    si: 'start_interval',
    ei: 'end_interval',
    tv: 'trading_volume'
  }
})
