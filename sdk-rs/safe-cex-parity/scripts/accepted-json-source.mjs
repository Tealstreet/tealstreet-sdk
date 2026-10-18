import assert from 'node:assert/strict';

// Keep authored JSON tokens outside the contract schema. Native probes must see
// the original key order and number spelling, even after JS validates a row.
const sources = new WeakMap();
const copiedProperties = new WeakMap();

export function parseJsonWithSource(text) {
  JSON.parse(text); // Validate the complete JSON grammar before recording spans.
  let offset = 0;
  const space = () => { while (/\s/.test(text[offset] ?? '') && offset < text.length) offset++; };
  function read() {
    space();
    const begin = offset;
    const token = text[offset++];
    let value;
    const node = {};
    if (token === '{' || token === '[') {
      value = token === '{' ? {} : [];
      node.fields = new Map();
      space();
      const close = token === '{' ? '}' : ']';
      while (text[offset] !== close) {
        let key;
        if (token === '{') {
          key = read().value;
          assert.ok(!node.fields.has(key), `accepted-set: duplicate JSON key: ${key}`);
          space(); offset++; // Colon; the complete parse already validated it.
        } else key = String(value.length);
        const child = read();
        Object.defineProperty(value, key, { value: child.value, enumerable: true, writable: true, configurable: true });
        node.fields.set(key, child);
        space();
        if (text[offset] !== ',') break;
        offset++; space();
      }
      offset++;
      node.array = Array.isArray(value);
      node.keys = Object.keys(value);
      sources.set(value, node);
    } else if (token === '"') {
      while (text[offset] !== '"') {
        if (text[offset] === '\\') offset++;
        offset++;
      }
      offset++;
      value = JSON.parse(text.slice(begin, offset));
    } else {
      while (offset < text.length && !/[\s,\]}]/.test(text[offset])) offset++;
      value = JSON.parse(text.slice(begin, offset));
    }
    node.value = value;
    node.raw = text.slice(begin, offset);
    return node;
  }
  return read().value;
}

function unchanged(value, node) {
  if (!node.fields) return Object.is(value, node.value);
  if (!value || typeof value !== 'object' || Array.isArray(value) !== node.array) return false;
  const keys = Object.keys(value);
  return keys.length === node.keys.length && keys.every((key, i) => key === node.keys[i] && unchanged(value[key], node.fields.get(key)));
}

// Generated multi-entry dispatch wraps a case input. Transfer its source token
// as well as its JS value, including primitive -0 or an out-of-range integer.
export function copyJsonSourceProperty(target, key, source, sourceKey = key) {
  const node = sources.get(source)?.fields?.get(sourceKey) ?? copiedProperties.get(source)?.get(sourceKey);
  if (node) {
    const fields = copiedProperties.get(target) ?? new Map();
    fields.set(key, node);
    copiedProperties.set(target, fields);
  }
  return target;
}

export function stringifyJsonWithSource(value) {
  const visiting = new Set();
  function write(value, original) {
    const node = original ?? (value && typeof value === 'object' ? sources.get(value) : undefined);
    if (node && unchanged(value, node)) return node.raw;
    if (!value || typeof value !== 'object') return Object.is(value, -0) ? '-0' : JSON.stringify(value);
    if (visiting.has(value)) throw new TypeError('Converting circular structure to JSON');
    visiting.add(value);
    const fields = copiedProperties.get(value);
    const member = key => write(value[key], fields?.get(key) ?? node?.fields?.get(key));
    const originalKeys = node?.fields ? [...node.fields.keys()] : [];
    const keys = [...originalKeys.filter(key => Object.hasOwn(value, key)), ...Object.keys(value).filter(key => !originalKeys.includes(key))];
    const result = Array.isArray(value)
      ? `[${Array.from({ length: value.length }, (_, i) => member(String(i)) ?? 'null').join(',')}]`
      : `{${keys.flatMap(key => {
        const raw = member(key);
        return raw === undefined ? [] : [`${JSON.stringify(key)}:${raw}`];
      }).join(',')}}`;
    visiting.delete(value);
    return result;
  }
  return write(value);
}
