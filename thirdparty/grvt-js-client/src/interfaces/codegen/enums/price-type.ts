// Selects which price feed to compare the alert threshold against
export enum EPriceType {
  // Mark price — the fair value price used for margin calculations
  MARK = 'MARK',
  // Last traded price
  LAST = 'LAST',
  // Index price — the underlying spot reference price
  INDEX = 'INDEX',
  // Mid price — mid-point of the best bid and ask
  MID = 'MID',
}

export const EPriceTypeInt: Record<EPriceType, number> = Object.freeze({
  [EPriceType.MARK]: 1,
  [EPriceType.LAST]: 2,
  [EPriceType.INDEX]: 3,
  [EPriceType.MID]: 4
})
