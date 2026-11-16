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

// Only this closed literal grammar is accepted; comments and quoted strings are
// tokenized before locating the declaration, so prose cannot supply live keys.
export function validateWasmBudgetSource(source) {
  const tokens = (source.match(/\/\*[\s\S]*?\*\/|\/\/[^\n\r]*|"(?:\\[\s\S]|[^"\\])*"|'(?:\\[\s\S]|[^'\\])*'|[a-zA-Z_$][\w$]*|\d[\d_]*|[^\s]/g) ?? [])
    .filter(token => !token.startsWith('//') && !token.startsWith('/*'));
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
  if (tokens[++index] !== ')') throw new Error('invalid budget literal boundary');
  exactNames(names, census.budgetVenues, 'budget venues');
}
