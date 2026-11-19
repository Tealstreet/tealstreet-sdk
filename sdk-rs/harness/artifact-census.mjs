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

// The generated cap file: exact fields, one entry per census family, KiB-aligned
// positive integers. Raw duplicates refuse before JSON.parse can pick a winner.
export function readWasmCaps(source) {
  const caps = parseUniqueJson(source);
  if (!caps || typeof caps !== 'object' || Array.isArray(caps)) throw new Error('invalid wasm caps file');
  exactNames(Object.keys(caps), ['schema', 'reserveBytes', 'combined', 'families'], 'wasm caps field');
  if (caps.schema !== 1) throw new Error(`unsupported wasm caps schema: ${caps.schema}`);
  if (caps.reserveBytes !== 10_240) throw new Error('wasm caps reserve must stay 10 KiB (rule 17)');
  if (!caps.families || typeof caps.families !== 'object' || Array.isArray(caps.families))
    throw new Error('missing wasm caps families');
  exactNames(Object.keys(caps.families), census.budgetVenues, 'budget venues');
  for (const [name, cap] of [['combined', caps.combined], ...Object.entries(caps.families)]) {
    if (!Number.isSafeInteger(cap) || cap <= 0 || cap % 1024) throw new Error(`invalid wasm cap: ${name}`);
  }
  return caps;
}
