export const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null;

export const requireRecord = (value: unknown): Record<string, unknown> => {
  if (!isRecord(value)) throw new Error('Expected a test fixture record');
  return value;
};

export const requireArray = (value: unknown): unknown[] => {
  if (!Array.isArray(value)) throw new Error('Expected a test fixture array');
  return value;
};
