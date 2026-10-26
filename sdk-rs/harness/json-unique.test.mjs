import assert from 'node:assert/strict';
import test from 'node:test';
import { parseUniqueJson } from './json-unique.mjs';

test('unique JSON preserves values, escaped strings and independent object scopes', () => {
  for (const text of [
    '{"x":"quoted \\"products\\": {} []","rows":[{"x":1},{"x":2}],"nested":{"x":3}}',
    '["x",1,true,null,{"y":"\\u0070roducts"}]',
    'null',
  ]) assert.deepEqual(parseUniqueJson(text), JSON.parse(text));
});
for (const text of [
  '{"products":[],"products":[]}',
  '{"products":[],"\\u0070roducts":[]}',
  '{"products":[{"price":"1","price":"2"}]}',
  '{"pagination":{"has_next":true,"has_next":false}}',
  '{"\\ud834\\udd1e":1,"𝄞":2}',
  '{"a":[{"x":1,"x":2}]}',
]) test(`rejects duplicate decoded keys in ${text}`, () => assert.throws(() => parseUniqueJson(text), /duplicate JSON/));

test('unique-key validation retains the native JSON grammar check', () => {
  for (const text of ['{"a":1,}', '[undefined]', '{"a":"unfinished}'])
    assert.throws(() => parseUniqueJson(text), SyntaxError);
});
