import { type SchemaPairMap } from './types'

// Schema map for the 'SUPPORTED_ASSET' struct.
export const SUPPORTED_ASSET_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    asset_id: 'ai',
    asset_code: 'ac',
    balance_decimals: 'bd'
  },
  LITE_TO_FULL: {
    ai: 'asset_id',
    ac: 'asset_code',
    bd: 'balance_decimals'
  }
})
