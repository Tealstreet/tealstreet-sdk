/** One captured action identity; callers validate its UUIDv7 timestamp at ingress. */
export type PlacementIntentIdentity = Readonly<{ id: string; createdAt: number }>;

export class PlacementIntentLedger<Result> {
  constructor(readonly capacity = 1024) {}

  run(_intent: PlacementIntentIdentity, _fingerprint: string, send: () => Promise<Result>): Promise<Result> {
    return send();
  }

  get size(): number { return 0; }
}
