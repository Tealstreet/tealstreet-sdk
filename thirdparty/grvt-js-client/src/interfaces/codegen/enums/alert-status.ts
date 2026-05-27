// Lifecycle state of an alert
export enum EAlertStatus {
  // Alert is enabled and will be evaluated on each price tick
  ACTIVE = 'ACTIVE',
  // Alert fired and was deactivated after the condition was met
  TRIGGERED = 'TRIGGERED',
  // Alert was automatically deactivated after its expiry_at time passed
  EXPIRED = 'EXPIRED',
  // Alert was manually deactivated by the user
  INACTIVE = 'INACTIVE',
}

export const EAlertStatusInt: Record<EAlertStatus, number> = Object.freeze({
  [EAlertStatus.ACTIVE]: 1,
  [EAlertStatus.TRIGGERED]: 2,
  [EAlertStatus.EXPIRED]: 3,
  [EAlertStatus.INACTIVE]: 4
})
