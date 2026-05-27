import { type SchemaPairMap } from './types'

// Schema map for the 'SPOT_BORROW_CONFIG' struct.
export const SPOT_BORROW_CONFIG_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    borrow_limit: 'bl',
    borrow_rate: 'br',
    collateral_limit: 'cl'
  },
  LITE_TO_FULL: {
    bl: 'borrow_limit',
    br: 'borrow_rate',
    cl: 'collateral_limit'
  }
})
