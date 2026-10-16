import assert from 'node:assert/strict';

const escaped = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

// Bounded existence precheck, not a replacement for the canonical Rust indexer.
function blockEnd(source, start) {
  let depth = 1;
  let string = false;
  let lineComment = false;
  let blockComment = 0;
  for (let i = start + 1; i < source.length; i++) {
    const pair = source.slice(i, i + 2);
    if (lineComment) {
      if (source[i] === '\n') lineComment = false;
    } else if (blockComment) {
      if (pair === '/*') { blockComment++; i++; }
      else if (pair === '*/') { blockComment--; i++; }
    } else if (string) {
      if (source[i] === '\\') i++;
      else if (source[i] === '"') string = false;
    } else if (pair === '//') { lineComment = true; i++; }
    else if (pair === '/*') { blockComment = 1; i++; }
    else if (source[i] === '"') string = true;
    else if (source[i] === '{') depth++;
    else if (source[i] === '}' && --depth === 0) return i;
  }
  assert.fail('unclosed Rust impl in target precheck');
}

export function splitQualifiedTarget(value) {
  const separator = value.indexOf('::');
  assert(separator > 0 && separator + 2 < value.length, 'invalid mapped target');
  return [value.slice(0, separator), value.slice(separator + 2)];
}

export function assertQualifiedTarget(source, name, label = name) {
  const parts = name.split('::');
  const leaf = parts.at(-1);
  assert(parts.every(part => /^[A-Za-z_][A-Za-z0-9_]*$/.test(part)), 'invalid target: ' + label);
  if (parts.length === 1) {
    assert(new RegExp('\\b(?:fn|struct|type)\\s+' + escaped(leaf) + '(?:\\s|<|\\()').test(source),
      'target must actually exist: ' + label);
    return;
  }
  if (parts.length === 2 && parts[0] === 'BoundHost') {
    let found = false;
    for (const match of source.matchAll(/\bimpl\b[^{}]*\{/g)) {
      const header = match[0];
      if (/\bfor\b/.test(header) || !/\bBoundHost\s*(?:<[^{}]*>)?\s*\{$/.test(header)) continue;
      const start = match.index + header.length - 1;
      const body = source.slice(start + 1, blockEnd(source, start));
      if (new RegExp('\\bpub\\s+fn\\s+' + escaped(leaf) + '(?:\\s|<|\\()').test(body)) found = true;
    }
    assert(found, 'public inherent owner mismatch: ' + label);
    return;
  }
  // The existing mapped ReadHost methods are trait declarations, not invented impl targets.
  if (parts.length === 2 && parts[0] === 'ReadHost') {
    const declaration = /\bpub\s+trait\s+ReadHost\b[^{}]*\{/.exec(source);
    assert(declaration, 'trait owner missing: ' + label);
    const start = declaration.index + declaration[0].length - 1;
    assert(new RegExp('\\bfn\\s+' + escaped(leaf) + '(?:\\s|<|\\()').test(source.slice(start + 1, blockEnd(source, start))),
      'trait method missing: ' + label);
    return;
  }
  assert.fail('unsupported qualified target precheck: ' + label);
}
