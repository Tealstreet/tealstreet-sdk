import { type SchemaPairMap } from './types'
import { REWARD_ACCOUNT_MULTIPLIER_METADATA_MAP } from './reward_account_multiplier_metadata'

// Schema map for the 'ACCOUNT_MULTIPLIER_OVERRIDE_ENTRY' struct.
export const ACCOUNT_MULTIPLIER_OVERRIDE_ENTRY_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    id: 'i',
    metric: 'm',
    multiplier: 'm1',
    metadata: ['m2', REWARD_ACCOUNT_MULTIPLIER_METADATA_MAP.FULL_TO_LITE],
    created_by_id: 'cb',
    effective_epoch_end: 'ee',
    create_time: 'ct',
    reserve_multiplier: 'rm'
  },
  LITE_TO_FULL: {
    i: 'id',
    m: 'metric',
    m1: 'multiplier',
    m2: ['metadata', REWARD_ACCOUNT_MULTIPLIER_METADATA_MAP.LITE_TO_FULL],
    cb: 'created_by_id',
    ee: 'effective_epoch_end',
    ct: 'create_time',
    rm: 'reserve_multiplier'
  }
})
