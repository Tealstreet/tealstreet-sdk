import { type SchemaPairMap } from './types'

// Schema map for the 'API_CREATE_PRICE_LEVEL_ALERT_REQUEST' struct.
export const API_CREATE_PRICE_LEVEL_ALERT_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    instrument: 'i',
    price_threshold: 'pt',
    triggered_by: 'tb',
    condition: 'c',
    note: 'n',
    frequency: 'f',
    alert_type: 'at'
  },
  LITE_TO_FULL: {
    i: 'instrument',
    pt: 'price_threshold',
    tb: 'triggered_by',
    c: 'condition',
    n: 'note',
    f: 'frequency',
    at: 'alert_type'
  }
})
