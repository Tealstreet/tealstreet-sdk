import { type SchemaPairMap } from './types'
import { NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP } from './non_liq_sub_account_config_v_2'

// Schema map for the 'API_GET_NLSA_CONFIG_RESPONSE' struct.
export const API_GET_NLSA_CONFIG_RESPONSE_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    enabled: 'e',
    sub_acc_configs: ['sa', [NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP.FULL_TO_LITE]]
  },
  LITE_TO_FULL: {
    e: 'enabled',
    sa: ['sub_acc_configs', [NON_LIQ_SUB_ACCOUNT_CONFIG_V_2_MAP.LITE_TO_FULL]]
  }
})
