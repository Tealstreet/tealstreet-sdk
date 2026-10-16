const raws = [null, 'null', 'false', 'true', '0', '-0', '1e400', '-1e400', '1e-400', '-1e-400', '""', '"0"', '"false"', '"\\ufeff"', '[]', '{}'];
for (let n = -200; n <= 200; n++) {
  for (const raw of [String(n), JSON.stringify(String(n)), `[${n}]`, `{"x":${n}}`]) raws.push(raw);
}
for (const raw of ['null', '[]', '{}', '""', '"\\ufeff"']) {
  raws.push(`[${raw}]`, `[[${raw}]]`, `{"toString":${raw}}`);
}

raws.push('[{"toString":null}]', '[[{"toString":null}]]', '{"valueOf":12,"toString":null}');
for (const key of ['valueOf', 'toString']) {
  for (const raw of ['null', 'false', '0', '"12"', '[]', '{}']) raws.push(`{"${key}":${raw}}`);
}
for (const text of ['', '0x10', '0b10', '0o10', '12abc', '-0', 'Infinity', '\ufeff12\ufeff', '\u008512', '1e400']) {
  raws.push(JSON.stringify(text), JSON.stringify([text]), JSON.stringify([[text]]));
}
function number(value) {
  try {
    const n = Number(value), unary = +value;
    if (!Object.is(n, unary)) throw new Error('Number and unary + disagree');
    return {ok: Number.isNaN(n) ? 'NaN' : Object.is(n, -0) ? '-0' : String(n)};
  } catch (error) { return {error: {name:error.name, message:error.message}}; }
}
process.stdout.write(JSON.stringify(raws.map(raw => {
  const value = raw === null ? undefined : JSON.parse(raw);
  return {raw, expected: Boolean(value), number: number(value)};
})));
