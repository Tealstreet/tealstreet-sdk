import { type SchemaPairMap } from './types'
import { API_ALERT_SUMMARY_MAP } from './api_alert_summary'

// Schema map for the 'API_LIST_ALERTS_RESPONSE' struct.
export const API_LIST_ALERTS_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    alerts: ['a', [API_ALERT_SUMMARY_MAP.FULL_TO_LITE]],
    next_cursor: 'nc'
  },
  LITE_TO_FULL: {
    a: ['alerts', [API_ALERT_SUMMARY_MAP.LITE_TO_FULL]],
    nc: 'next_cursor'
  }
})
