import { type SchemaPairMap } from './types'

// Schema map for the 'SPOT_BALANCE' struct.
export const SPOT_BALANCE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    currency: 'c',
    balance: 'b',
    index_price: 'ip',
    entry_price: 'ep',
    realized_pnl: 'rp',
    unrealized_pnl: 'up',
    available_to_transfer: 'at'
  },
  LITE_TO_FULL: {
    c: 'currency',
    b: 'balance',
    ip: 'index_price',
    ep: 'entry_price',
    rp: 'realized_pnl',
    up: 'unrealized_pnl',
    at: 'available_to_transfer'
  }
})
