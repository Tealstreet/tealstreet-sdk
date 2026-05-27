import { type SchemaPairMap } from './types'
import { MARGIN_MATRIX_RELAXATIONS_MAP } from './margin_matrix_relaxations'
import { TRANSFER_CHECK_RELAXATIONS_MAP } from './transfer_check_relaxations'
import { LIQUIDATION_RELAXATIONS_MAP } from './liquidation_relaxations'
import { DERISK_RELAXATIONS_MAP } from './derisk_relaxations'

// Schema map for the 'NON_LIQ_SUB_ACCOUNT_CAPABILITIES' struct.
export const NON_LIQ_SUB_ACCOUNT_CAPABILITIES_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    margin_matrix_relaxations: ['mm', MARGIN_MATRIX_RELAXATIONS_MAP.FULL_TO_LITE],
    transfer_check_relaxations: ['tc', TRANSFER_CHECK_RELAXATIONS_MAP.FULL_TO_LITE],
    liquidation_relaxations: ['lr', LIQUIDATION_RELAXATIONS_MAP.FULL_TO_LITE],
    derisk_relaxations: ['dr', DERISK_RELAXATIONS_MAP.FULL_TO_LITE]
  },
  LITE_TO_FULL: {
    mm: ['margin_matrix_relaxations', MARGIN_MATRIX_RELAXATIONS_MAP.LITE_TO_FULL],
    tc: ['transfer_check_relaxations', TRANSFER_CHECK_RELAXATIONS_MAP.LITE_TO_FULL],
    lr: ['liquidation_relaxations', LIQUIDATION_RELAXATIONS_MAP.LITE_TO_FULL],
    dr: ['derisk_relaxations', DERISK_RELAXATIONS_MAP.LITE_TO_FULL]
  }
})
