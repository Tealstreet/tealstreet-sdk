/** Request header names are case insensitive; duplicates cannot authenticate by last-wins lookup. */
export const uniqueRequestHeaders = (headers = {}) => {
  const entries = Array.isArray(headers) ? headers : typeof headers?.entries === 'function' ? [...headers.entries()] : Object.entries(headers ?? {});
  const out = Object.create(null);
  for (const [name, value] of entries) {
    if (typeof name !== 'string' || !name) throw new Error('Invalid request header name');
    const key = name.toLowerCase();
    if (Object.hasOwn(out, key)) throw new Error('Duplicate request header: ' + key);
    out[key] = String(value);
  }
  return out;
};
