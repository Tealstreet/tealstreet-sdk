// The list of asset kinds that are supported on the GRVT exchange
export enum EKind {
  // the perpetual asset kind
  PERPETUAL = 'PERPETUAL',
  // the future asset kind
  FUTURE = 'FUTURE',
  // the call option asset kind
  CALL = 'CALL',
  // the put option asset kind
  PUT = 'PUT',
  // the spot swap asset kind
  SPOT_SWAP = 'SPOT_SWAP',
}

export const EKindInt: Record<EKind, number> = Object.freeze({
  [EKind.PERPETUAL]: 1,
  [EKind.FUTURE]: 2,
  [EKind.CALL]: 3,
  [EKind.PUT]: 4,
  [EKind.SPOT_SWAP]: 8
})
