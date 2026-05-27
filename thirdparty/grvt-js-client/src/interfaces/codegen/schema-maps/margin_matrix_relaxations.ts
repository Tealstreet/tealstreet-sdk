import { type SchemaPairMap } from './types'

// Schema map for the 'MARGIN_MATRIX_RELAXATIONS' struct.
export const MARGIN_MATRIX_RELAXATIONS_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    relax_pre_order_check: 'rp',
    relax_post_order_check: 'rp1',
    relax_pre_trade_check: 'rp2',
    relax_post_trade_check: 'rp3'
  },
  LITE_TO_FULL: {
    rp: 'relax_pre_order_check',
    rp1: 'relax_post_order_check',
    rp2: 'relax_pre_trade_check',
    rp3: 'relax_post_trade_check'
  }
})
