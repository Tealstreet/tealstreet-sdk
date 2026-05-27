/**
 * If value is a JSON string representing an object/array, parse it back.
 * Returns the original value if it's not a string or not valid JSON.
 *
 * Used to undo axios transformRequest stringification so that signing
 * interceptors receive clean objects.
 */
export const tryParseJson = (value: unknown): unknown => {
  if (typeof value !== 'string') return value;
  try {
    const parsed = JSON.parse(value);
    return typeof parsed === 'object' && parsed !== null ? parsed : value;
  } catch {
    return value;
  }
};
