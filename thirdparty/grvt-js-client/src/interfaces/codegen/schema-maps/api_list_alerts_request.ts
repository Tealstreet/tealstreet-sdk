import { type SchemaPairMap } from './types'

// Schema map for the 'API_LIST_ALERTS_REQUEST' struct.
export const API_LIST_ALERTS_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    instrument: 'i',
    is_active: 'ia',
    limit: 'l',
    cursor: 'c'
  },
  LITE_TO_FULL: {
    i: 'instrument',
    ia: 'is_active',
    l: 'limit',
    c: 'cursor'
  }
})
