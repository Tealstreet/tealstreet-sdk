import { type SchemaPairMap } from './types'
import { MINI_POSITION_MAP } from './mini_position'

// Schema map for the 'API_GET_ALL_POSITIONS_BY_ASSET_RESPONSE' struct.
export const API_GET_ALL_POSITIONS_BY_ASSET_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    result: ['r', [MINI_POSITION_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    r: ['result', [MINI_POSITION_MAP.LITE_TO_FULL]]
  }
})
