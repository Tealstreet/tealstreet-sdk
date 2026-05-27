import { type SchemaPairMap } from './types'

// Schema map for the 'API_DELETE_ALL_ALERTS_RESPONSE' struct.
export const API_DELETE_ALL_ALERTS_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    deleted_count: 'dc'
  },
  LITE_TO_FULL: {
    dc: 'deleted_count'
  }
})
