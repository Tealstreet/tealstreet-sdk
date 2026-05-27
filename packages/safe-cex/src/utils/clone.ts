/**
 * Check if value is a plain object (not a special type like Date, Map, etc.)
 */
const isPlainObject = (value: unknown): value is Record<string, unknown> => {
  if (value === null || typeof value !== 'object') return false;
  const proto = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
};

/**
 * Fast recursive clone for plain objects and arrays.
 * Returns null if it encounters a non-plain object (Date, Map, Set, etc.)
 * to signal that structuredClone should be used instead.
 */
const fastClone = <T>(value: T): T | null => {
  // Primitives and null
  if (value === null || typeof value !== 'object') {
    return value;
  }

  // Arrays
  if (Array.isArray(value)) {
    const len = value.length;
    const result = new Array(len);
    for (let i = 0; i < len; i++) {
      const cloned = fastClone(value[i]);
      if (cloned === null && value[i] !== null) {
        return null; // Signal to use structuredClone
      }
      result[i] = cloned;
    }
    return result as T;
  }

  // Only handle plain objects - bail out for Date, Map, Set, etc.
  if (!isPlainObject(value)) {
    return null; // Signal to use structuredClone
  }

  // Plain objects
  const result: Record<string, unknown> = {};
  const keys = Object.keys(value);
  for (let i = 0; i < keys.length; i++) {
    const key = keys[i];
    const cloned = fastClone(value[key]);
    if (cloned === null && value[key] !== null) {
      return null; // Signal to use structuredClone
    }
    result[key] = cloned;
  }
  return result as T;
};

/**
 * Deep clone an object. Uses fast recursive clone for plain objects/arrays,
 * falls back to structuredClone for complex types (Date, Map, Set, etc.)
 */
export const clone = <K extends Record<string, any>>(obj: K): K => {
  const result = fastClone(obj);
  if (result !== null) {
    return result;
  }

  // Fallback for complex types
  if (typeof structuredClone !== 'undefined') {
    return structuredClone(obj);
  }
  return JSON.parse(JSON.stringify(obj));
};
