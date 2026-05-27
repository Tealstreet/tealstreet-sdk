// How many times an alert can fire before being deactivated
export enum EAlertFrequency {
  // Alert fires once and is automatically deactivated
  ONCE = 'ONCE',
}

export const EAlertFrequencyInt: Record<EAlertFrequency, number> = Object.freeze({
  [EAlertFrequency.ONCE]: 1
})
