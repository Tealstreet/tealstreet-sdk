export enum EPositionCloseStatus {
  // Position fully closed via reducing trade or flip
  CLOSED = 'CLOSED',
  // Position closed via liquidation
  LIQUIDATED = 'LIQUIDATED',
  // Position closed via settlement
  SETTLED = 'SETTLED',
  // Position partially closed
  PARTIALLY_CLOSED = 'PARTIALLY_CLOSED',
}

export const EPositionCloseStatusInt: Record<EPositionCloseStatus, number> = Object.freeze({
  [EPositionCloseStatus.CLOSED]: 1,
  [EPositionCloseStatus.LIQUIDATED]: 2,
  [EPositionCloseStatus.SETTLED]: 3,
  [EPositionCloseStatus.PARTIALLY_CLOSED]: 4
})
