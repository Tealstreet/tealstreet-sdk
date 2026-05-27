export const isNotNumber = (v: unknown): v is null | undefined =>
  Array.isArray(v) || typeof v === 'boolean' || v === '' || isNaN(Number(v)) || v === null || v === undefined;
