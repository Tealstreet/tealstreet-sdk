import { readFileSync } from 'node:fs';

// JSON.parse loses duplicate members, including escaped spellings of the same key.
export function parseUniqueJson(source) {
  const value = JSON.parse(source);
  const stack = [];
  for (const token of source.match(/"(?:\\[\s\S]|[^"\\])*"|[{}\[\],]/g) ?? []) {
    if (token === '{' || token === '[') {
      stack.push({ object: token === '{', key: true, names: new Set() });
    } else if (token === '}' || token === ']') {
      stack.pop();
    } else if (token === ',') {
      if (stack.at(-1)?.object) stack.at(-1).key = true;
    } else if (stack.at(-1)?.object && stack.at(-1).key) {
      const frame = stack.at(-1);
      const name = JSON.parse(token);
      if (frame.names.has(name)) throw new Error(`duplicate JSON field: ${name}`);
      frame.names.add(name);
      frame.key = false;
    }
  }
  return value;
}

function exactNames(actual, expected, label) {
  const seen = new Set();
  for (const name of actual) {
    if (seen.has(name)) throw new Error(`duplicate ${label}: ${name}`);
    seen.add(name);
  }
  const missing = expected.filter(name => !seen.has(name));
  const unexpected = actual.filter(name => !expected.includes(name));
  if (missing.length) throw new Error(`missing ${label}: ${missing.join(', ')}`);
  if (unexpected.length) throw new Error(`unexpected ${label}: ${unexpected.join(', ')}`);
}

const census = parseUniqueJson(readFileSync(new URL('../artifact-census.json', import.meta.url), 'utf8'));
exactNames(Object.keys(census), ['budgetVenues', 'constructionPolicyVenues'], 'census field');
for (const [label, names] of Object.entries(census)) {
  if (!Array.isArray(names) || !names.length || names.some(name => typeof name !== 'string' || !/^[a-z][a-z0-9-]*$/.test(name)))
    throw new Error(`invalid ${label} census`);
  exactNames(names, [...new Set(names)], label);
  if (names.join(',') !== [...names].sort().join(',')) throw new Error(`unsorted ${label} census`);
}

export function readConstructionPolicy(source) {
  const rules = parseUniqueJson(source);
  if (!rules?.venues || typeof rules.venues !== 'object' || Array.isArray(rules.venues))
    throw new Error('missing construction policy venues');
  exactNames(Object.keys(rules.venues), census.constructionPolicyVenues, 'construction policy venues');
  return rules;
}

// This budget file accepts strings, comments and nested templates, but no regex
// or division syntax. Rejecting unsupported slash tokens keeps boundaries closed.
function budgetTokens(source) {
  let index = 0;
  function quoted(quote) {
    const start = index++;
    while (index < source.length) {
      const char = source[index++];
      if (char === '\\') index++;
      else if (char === quote) return source.slice(start, index);
    }
    throw new Error('unterminated budget string');
  }
  function template() {
    index++;
    while (index < source.length) {
      const char = source[index++];
      if (char === '\\') index++;
      else if (char === '`') return;
      else if (char === '$' && source[index] === '{') {
        index++;
        code(true);
      }
    }
    throw new Error('unterminated budget template');
  }
  function code(interpolation = false) {
    const tokens = [];
    let depth = interpolation ? 1 : 0;
    while (index < source.length) {
      const char = source[index];
      if (/\s/.test(char)) { index++; continue; }
      if (source.startsWith('//', index)) {
        while (index < source.length && !/[\n\r\u2028\u2029]/.test(source[index])) index++;
        continue;
      }
      if (source.startsWith('/*', index)) {
        const end = source.indexOf('*/', index + 2);
        if (end === -1) throw new Error('unterminated budget comment');
        index = end + 2;
        continue;
      }
      if (char === '/') throw new Error('unsupported budget slash syntax');
      if (char === '"' || char === "'") { tokens.push(quoted(char)); continue; }
      if (char === '`') { template(); tokens.push('<template>'); continue; }
      if (interpolation && char === '{') depth++;
      if (interpolation && char === '}' && --depth === 0) { index++; return tokens; }
      const token = source.slice(index).match(/^(?:[a-zA-Z_$][\w$]*|\d[\d_]*)/)?.[0] ?? char;
      tokens.push(token);
      index += token.length;
    }
    if (interpolation) throw new Error('unterminated budget interpolation');
    return tokens;
  }
  return code();
}

// Only this closed literal grammar is accepted for the live export; inert
// template text and its recursively scanned expressions cannot impersonate it.
export function validateWasmBudgetSource(source) {
  const tokens = budgetTokens(source);
  const declaration = ['export', 'const', 'WASM_VENUE_OVERRIDES', '=', 'Object', '.', 'freeze', '(', '{'];
  const starts = tokens.flatMap((_, index) => declaration.every((token, offset) => tokens[index + offset] === token) ? [index + declaration.length] : []);
  if (starts.length !== 1) throw new Error('expected one literal WASM_VENUE_OVERRIDES declaration');
  let index = starts[0];
  const names = [];
  while (tokens[index] !== '}') {
    const key = tokens[index++];
    if (!/^(?:[a-z][a-z0-9-]*|'[a-z][a-z0-9-]*'|"[a-z][a-z0-9-]*")$/.test(key ?? ''))
      throw new Error('invalid literal budget venue');
    const name = key.replace(/^['"]|['"]$/g, '');
    if (names.includes(name)) throw new Error(`duplicate budget venue: ${name}`);
    names.push(name);
    if (tokens[index++] !== ':') throw new Error(`missing budget value: ${name}`);
    const amount = tokens[index++];
    if (!/^[1-9][\d_]*$/.test(amount ?? '') || !Number.isSafeInteger(Number(amount.replaceAll('_', ''))))
      throw new Error(`invalid literal budget value: ${name}`);
    if (tokens[index] === ',') index++;
    else if (tokens[index] !== '}') throw new Error(`invalid budget separator: ${name}`);
  }
  if (tokens[++index] !== ')' || tokens[++index] !== ';') throw new Error('invalid budget literal boundary');
  exactNames(names, census.budgetVenues, 'budget venues');
}
