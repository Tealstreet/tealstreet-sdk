import { type SchemaPairMap } from './types'
import { SUPPORTED_ASSET_MAP } from './supported_asset'

// Schema map for the 'FUTURES_WALLET_ASSETS' struct.
export const FUTURES_WALLET_ASSETS_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    mode: 'm',
    supported_assets: ['sa', [SUPPORTED_ASSET_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    m: 'mode',
    sa: ['supported_assets', [SUPPORTED_ASSET_MAP.LITE_TO_FULL]]
  }
})
