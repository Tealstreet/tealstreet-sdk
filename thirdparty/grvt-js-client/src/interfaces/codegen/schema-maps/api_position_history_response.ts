import { type SchemaPairMap } from './types'
import { API_POSITION_HISTORY_MAP } from './api_position_history'

// Schema map for the 'API_POSITION_HISTORY_RESPONSE' struct.
export const API_POSITION_HISTORY_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', [API_POSITION_HISTORY_MAP.FULL_TO_LITE]],
    next: 'n'
  },
  LITE_TO_FULL: {
    r: ['result', [API_POSITION_HISTORY_MAP.LITE_TO_FULL]],
    n: 'next'
  }
})
