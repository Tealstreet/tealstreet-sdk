import { type SchemaPairMap } from './types'

// Schema map for the 'TRANSFER_CHECK_RELAXATIONS' struct.
export const TRANSFER_CHECK_RELAXATIONS_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    relax_available_balance_check: 'ra',
    waive_spot_balance_check: 'ws'
  },
  LITE_TO_FULL: {
    ra: 'relax_available_balance_check',
    ws: 'waive_spot_balance_check'
  }
})
