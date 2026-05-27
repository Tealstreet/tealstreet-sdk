import { type SchemaPairMap } from './types'

// Schema map for the 'DERISK_RELAXATIONS' struct.
export const DERISK_RELAXATIONS_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    exempt_derisk: 'ed'
  },
  LITE_TO_FULL: {
    ed: 'exempt_derisk'
  }
})
