import { type SchemaPairMap } from './types'
import { NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP } from './non_liq_sub_account_config_v_2'

// Schema map for the 'API_SET_NLSA_CONFIG_REQUEST' struct.
export const API_SET_NLSA_CONFIG_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    enabled: 'e',
    add_or_edit: ['ao', [NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP.FULL_TO_LITE]],
    remove_sub_acc_i_ds: 'rs'
  },
  LITE_TO_FULL: {
    e: 'enabled',
    ao: ['add_or_edit', [NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP.LITE_TO_FULL]],
    rs: 'remove_sub_acc_i_ds'
  }
})
