import { type SchemaPairMap } from './types'

// Schema map for the 'API_ALERT_SUMMARY' struct.
export const API_ALERT_SUMMARY_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    id: 'i',
    instrument: 'i1',
    price_threshold: 'pt',
    triggered_by: 'tb',
    is_active: 'ia',
    condition: 'c',
    status: 's',
    note: 'n'
  },
  LITE_TO_FULL: {
    i: 'id',
    i1: 'instrument',
    pt: 'price_threshold',
    tb: 'triggered_by',
    ia: 'is_active',
    c: 'condition',
    s: 'status',
    n: 'note'
  }
})
