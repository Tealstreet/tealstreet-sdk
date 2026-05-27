export enum ESubAccountMode {
  // Single asset mode: the subaccount is only allowed to hold one asset as collateral
  SINGLE_ASSET_MODE = 'SINGLE_ASSET_MODE',
}

export const ESubAccountModeInt: Record<ESubAccountMode, number> = Object.freeze({
  [ESubAccountMode.SINGLE_ASSET_MODE]: 1
})
