export const normalizeEpochToSeconds = (epoch: number): number => {
  // If the epoch is too large, assume ms
  return epoch > 1_000_000_000_000 ? Math.floor(epoch / 1000) : epoch;
};
