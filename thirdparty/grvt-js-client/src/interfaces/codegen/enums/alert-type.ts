// Discriminates the alert subtype and determines which detail struct applies
export enum EAlertType {
  // Triggers when a price feed reaches, crosses above, or crosses below a configured threshold
  PRICE_LEVEL = 'PRICE_LEVEL',
}

export const EAlertTypeInt: Record<EAlertType, number> = Object.freeze({
  [EAlertType.PRICE_LEVEL]: 1
})
