import { type SchemaPairMap } from './types'
import { NON_LIQ_SUB_ACCOUNT_CAPABILITIES_MAP } from './non_liq_sub_account_capabilities'

// Schema map for the 'NON_LIQ_SUB_ACCOUNT_CONFIG_V_2' struct.
export const NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    sub_acc_id: 'sa',
    capabilities: ['c', NON_LIQ_SUB_ACCOUNT_CAPABILITIES_MAP.FULL_TO_LITE]
  },
  LITE_TO_FULL: {
    sa: 'sub_acc_id',
    c: ['capabilities', NON_LIQ_SUB_ACCOUNT_CAPABILITIES_MAP.LITE_TO_FULL]
  }
})
