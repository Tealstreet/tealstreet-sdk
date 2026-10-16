const raws = ['null', 'false', 'true', '0', '-0', '1', '-1', '1e400', '-1e400', '1e-400', '-1e-400', '""', '"0"', '"false"', '" "', '[]', '{}', '[null]', '"\\ufeff"', '"\\u0085"', '"😀"'];
for (let exponent = -400; exponent <= 400; exponent++) {
  raws.push(`1e${exponent}`, `-1e${exponent}`);
}
process.stdout.write(JSON.stringify(raws.map(raw => {
  const value = JSON.parse(raw);
  return { raw, truthy: Boolean(value) };
})));
