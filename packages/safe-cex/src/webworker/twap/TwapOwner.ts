/** Host-owned identity; generation changes on logout, account switch or session revocation. */
export type TwapOwner = Readonly<{ ownerId: string; generation: number }>;

/** Multi-user hosts supply this before constructing their ExchangeWorker. */
export type TwapWorkerContext = Worker & { getTwapOwner?: () => TwapOwner | null };

/** Copy the snapshot: a host may update its live owner object during a wait. */
export const captureTwapOwner = (getOwner?: () => TwapOwner | null): TwapOwner | null | undefined => {
  if (!getOwner) return undefined;
  try {
    const owner = getOwner();
    return owner &&
      typeof owner.ownerId === 'string' &&
      owner.ownerId.length > 0 &&
      Number.isSafeInteger(owner.generation) &&
      owner.generation >= 0
      ? { ownerId: owner.ownerId, generation: owner.generation }
      : null;
  } catch {
    return null;
  }
};
