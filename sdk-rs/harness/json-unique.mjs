// JSON.parse validates grammar; decoded keys retain object scopes so neither
// a first-wins nor a last-wins consumer can hide conflicting source fields.
export function parseUniqueJson(text) {
  const value = JSON.parse(text);
  const tokens = text.match(/"(?:\\.|[^"\\])*"|[{}\[\]:,]/g) ?? [];
  const scopes = [];
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token === '{') scopes.push(new Set());
    else if (token === '[') scopes.push(null);
    else if (token === '}' || token === ']') scopes.pop();
    else if (token.startsWith('"') && tokens[i + 1] === ':') {
      const key = JSON.parse(token), keys = scopes.at(-1);
      if (keys?.has(key)) throw new Error('duplicate JSON property');
      keys?.add(key);
    }
  }
  return value;
}
