// Keep signed integer parameters exact when a venue uses nanosecond cursors.
export const parseRequestJson = (text) => JSON.parse(text, (_key, value, context) =>
  typeof value === 'number' && !Number.isSafeInteger(value) && /^-?\d+$/.test(context.source)
    ? BigInt(context.source) : value);
