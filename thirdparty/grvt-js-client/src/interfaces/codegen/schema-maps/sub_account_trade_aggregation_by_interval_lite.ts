import { type SchemaPairMap } from './types'

// Schema map for the 'SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE' struct.
export const SUB_ACCOUNT_TRADE_AGGREGATION_BY_INTERVAL_LITE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    start_interval: 'si',
    total_trade_volume: 'tt'
  },
  LITE_TO_FULL: {
    si: 'start_interval',
    tt: 'total_trade_volume'
  }
})
