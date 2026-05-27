import { type SchemaPairMap } from './types'

// Schema map for the 'API_GET_FUNDING_ACCOUNT_POSITION_SUMMARY_REQUEST' struct.
export const API_GET_FUNDING_ACCOUNT_POSITION_SUMMARY_REQUEST_MAP: SchemaPairMap = Object.freeze({
  FULL_TO_LITE: {
    main_account_i_ds: 'ma',
    assets: 'a'
  },
  LITE_TO_FULL: {
    ma: 'main_account_i_ds',
    a: 'assets'
  }
})
