import { type SchemaPairMap } from './types'
import { SUPPORTED_ASSET_MAP } from './supported_asset'
import { FUTURES_WALLET_ASSETS_MAP } from './futures_wallet_assets'

// Schema map for the 'API_GET_SUPPORTED_ASSETS_RESPONSE' struct.
export const API_GET_SUPPORTED_ASSETS_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    funding: ['f', [SUPPORTED_ASSET_MAP.FULL_TO_LITE]],
    spot: ['s', [SUPPORTED_ASSET_MAP.FULL_TO_LITE]],
    futures: ['f1', [FUTURES_WALLET_ASSETS_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    f: ['funding', [SUPPORTED_ASSET_MAP.LITE_TO_FULL]],
    s: ['spot', [SUPPORTED_ASSET_MAP.LITE_TO_FULL]],
    f1: ['futures', [FUTURES_WALLET_ASSETS_MAP.LITE_TO_FULL]]
  }
})
