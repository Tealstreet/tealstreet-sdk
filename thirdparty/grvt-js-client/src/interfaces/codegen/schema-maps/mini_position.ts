import { type SchemaPairMap } from './types'

// Schema map for the 'MINI_POSITION' struct.
export const MINI_POSITION_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_id: 'sa',
    balance: 'b',
    entry_price: 'ep',
    cumulative_realized_pnl: 'cr',
    cumulative_realized_funding_payment: 'cr1',
    margin_type: 'mt',
    leverage: 'l',
    position_balance: 'pb'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_id',
    b: 'balance',
    ep: 'entry_price',
    cr: 'cumulative_realized_pnl',
    cr1: 'cumulative_realized_funding_payment',
    mt: 'margin_type',
    l: 'leverage',
    pb: 'position_balance'
  }
})
