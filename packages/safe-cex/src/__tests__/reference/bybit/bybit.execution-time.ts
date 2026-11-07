const BYBIT_MS_THRESHOLD = 100_000_000_000;
const BYBIT_US_THRESHOLD = 1_000_000_000_000_000;

export const normalizeBybitExecutionTimestampMs = (raw: unknown): number | undefined => {
  if (raw === undefined || raw === null || raw === '') return undefined;

  const value = Number(raw);
  if (!Number.isFinite(value) || value <= 0) return undefined;

  if (value >= BYBIT_US_THRESHOLD) return Math.floor(value / 1000);
  if (value > BYBIT_MS_THRESHOLD) return Math.floor(value);
  return Math.floor(value * 1000);
};

export const normalizeBybitExecutionTimestampSeconds = (raw: unknown): number | undefined => {
  const timestampMs = normalizeBybitExecutionTimestampMs(raw);
  return timestampMs === undefined ? undefined : Math.floor(timestampMs / 1000);
};
