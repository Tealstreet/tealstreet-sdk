export enum EWalletType {
  // Funding wallet
  FUNDING = 'FUNDING',
  // Spot wallet
  SPOT = 'SPOT',
  // Futures wallet
  FUTURES = 'FUTURES',
}

export const EWalletTypeInt: Record<EWalletType, number> = Object.freeze({
  [EWalletType.FUNDING]: 1,
  [EWalletType.SPOT]: 2,
  [EWalletType.FUTURES]: 3
})
