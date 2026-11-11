import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Tokenize strings/comments before balancing test items: fixture strings can contain braces.
const rustTokens =
  /#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]|\/\/[^\n]*|\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])'|[{};,[\]()]|[^#\/"'{};,[\]()]+|./gs;
const branches = /if\s+[^{}]+\{\s*"([^"\\]*)"\s*\}\s*else\s*\{\s*"([^"\\]*)"\s*\}/;

const productionSource = (source: string): string => {
  const kept: string[] = [];
  let skipping = false;
  let depth = 0;
  let parentheses = 0;
  for (const token of source.match(rustTokens) ?? []) {
    if (token.startsWith('//') || token.startsWith('/*')) continue;
    if (/^#\s*\[/.test(token)) {
      skipping = true;
      continue;
    }
    if (!skipping) {
      kept.push(token);
    } else if (token === '{') {
      depth += 1;
    } else if (token === '(' || token === '[') {
      parentheses += 1;
    } else if (token === ')' || token === ']') {
      parentheses -= 1;
    } else if (token === '}') {
      if (depth === 0) kept.push(token);
      else depth -= 1;
      if (depth === 0) skipping = false;
    } else if ((token === ';' || token === ',') && depth === 0 && parentheses === 0) {
      skipping = false;
    }
  }
  return kept.join('');
};

export const rustRestUrls = (source: string): string[] => {
  const production = productionSource(source);
  const urls = new Set<string>();
  for (const match of production.matchAll(/"(https?:\/\/[^"\\]+)"/g)) {
    const literal = match[1]!;
    let expanded = [literal];
    // Only placeholders inside a fixed DNS name need expansion; path/signing placeholders do not.
    for (const placeholder of literal.matchAll(/\{(\w*)\}(?=[^/]*\.)/g)) {
      const name = placeholder[1]!;
      const expression = name
        ? new RegExp(`\\blet\\s+${name}\\s*=\\s*(${branches.source})`).exec(production)?.[1]
        : production.slice(match.index! + match[0].length).match(/^\s*,\s*(if[\s\S]*)/)?.[1];
      const values = expression?.match(branches);
      if (!values) throw new Error(`Unresolved REST origin template: ${literal}`);
      expanded = expanded.flatMap((url) => [values[1]!, values[2]!].map((value) => url.replace(placeholder[0], value)));
    }
    for (const url of expanded) {
      const origin = url.match(/^https?:\/\/[a-z0-9-]+(?:\.[a-z0-9-]+)+(?::\d+)?(?=[/{?]|$)/i)?.[0];
      // Bare schemes and caller-provided signing hosts do not declare a fixed venue origin.
      if (!origin) continue;
      const path = url.slice(origin.length).split(/[?{]/)[0]!;
      const base = origin + path;
      urls.add(base.endsWith('/') ? base : `${base}/`);
    }
  }
  return [...urls].sort();
};

export const engineRestUrls = (venues: string): Map<string, string[]> => {
  const result = new Map<string, string[]>();
  for (const crate of readdirSync(venues, { withFileTypes: true }).filter((entry) => entry.isDirectory())) {
    const src = resolve(venues, crate.name, 'src');
    const urls = new Set<string>();
    for (const file of readdirSync(src, { recursive: true, encoding: 'utf8' })) {
      if (!file.endsWith('.rs') || /(?:^|[/_])tests?(?:[/_.]|$)/.test(file)) continue;
      for (const url of rustRestUrls(readFileSync(resolve(src, file), 'utf8'))) urls.add(url);
    }
    if (!urls.size) throw new Error(`No REST URLs derived for venue crate ${crate.name}`);
    result.set(crate.name, [...urls].sort());
  }
  if (!result.size) throw new Error('No venue crates found');
  return result;
};
