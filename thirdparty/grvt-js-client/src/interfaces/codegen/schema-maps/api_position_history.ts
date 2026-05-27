import { type SchemaPairMap } from './types'

// Schema map for the 'API_POSITION_HISTORY' struct.
export const API_POSITION_HISTORY_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_account_id: 'sa',
    instrument: 'i',
    open_time: 'ot',
    status: 's',
    is_long: 'il',
    margin_type: 'mt',
    close_time: 'ct',
    entry_price: 'ep',
    exit_price: 'ep1',
    position_close_mark_price: 'pc',
    realized_pnl: 'rp',
    cumulative_fee: 'cf',
    cumulative_realized_funding_payment: 'cr',
    closed_volume_base: 'cv',
    closed_volume_quote: 'cv1',
    max_open_interest_base: 'mo',
    max_open_interest_quote: 'mo1',
    cumulative_initial_margin: 'ci',
    max_initial_margin: 'mi',
    leverage: 'l',
    unrealized_pnl: 'up'
  },
  LITE_TO_FULL: {
    sa: 'sub_account_id',
    i: 'instrument',
    ot: 'open_time',
    s: 'status',
    il: 'is_long',
    mt: 'margin_type',
    ct: 'close_time',
    ep: 'entry_price',
    ep1: 'exit_price',
    pc: 'position_close_mark_price',
    rp: 'realized_pnl',
    cf: 'cumulative_fee',
    cr: 'cumulative_realized_funding_payment',
    cv: 'closed_volume_base',
    cv1: 'closed_volume_quote',
    mo: 'max_open_interest_base',
    mo1: 'max_open_interest_quote',
    ci: 'cumulative_initial_margin',
    mi: 'max_initial_margin',
    l: 'leverage',
    up: 'unrealized_pnl'
  }
})
