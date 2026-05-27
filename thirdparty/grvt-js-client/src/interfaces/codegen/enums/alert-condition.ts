// Directional condition that determines when a price-level alert fires
export enum EAlertCondition {
  // Fire when current price >= threshold (price reaches or exceeds the level)
  GTE = 'GTE',
  // Fire when current price <= threshold (price reaches or drops to the level)
  LTE = 'LTE',
  // Fire only when price crosses upward through the threshold
  CROSS_UP = 'CROSS_UP',
  // Fire only when price crosses downward through the threshold
  CROSS_DOWN = 'CROSS_DOWN',
}

export const EAlertConditionInt: Record<EAlertCondition, number> = Object.freeze({
  [EAlertCondition.GTE]: 1,
  [EAlertCondition.LTE]: 2,
  [EAlertCondition.CROSS_UP]: 3,
  [EAlertCondition.CROSS_DOWN]: 4
})
