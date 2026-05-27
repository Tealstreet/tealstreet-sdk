import { type SchemaPairMap } from './types'

// Schema map for the 'LIQUIDATION_RELAXATIONS' struct.
export const LIQUIDATION_RELAXATIONS_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    exempt_liquidation: 'el'
  },
  LITE_TO_FULL: {
    el: 'exempt_liquidation'
  }
})
