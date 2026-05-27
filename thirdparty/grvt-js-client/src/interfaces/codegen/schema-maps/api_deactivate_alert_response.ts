import { type SchemaPairMap } from './types'

// Schema map for the 'API_DEACTIVATE_ALERT_RESPONSE' struct.
export const API_DEACTIVATE_ALERT_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    id: 'i',
    is_active: 'ia'
  },
  LITE_TO_FULL: {
    i: 'id',
    ia: 'is_active'
  }
})
