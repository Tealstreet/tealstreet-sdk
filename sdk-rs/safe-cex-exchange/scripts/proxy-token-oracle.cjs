// Execute actual expiry functions with injected decoded values, or real JWT decoding in --codec mode.
const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || '../../../node_modules/typescript');
const root = path.resolve(__dirname, '../../..');
const extract = (file, name) => {
  const source = fs.readFileSync(path.join(root, 'packages/safe-cex/src', file), 'utf8');
  const ast = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
  const declaration = ast.statements.find(node => ts.isFunctionDeclaration(node) && node.name?.text === name);
  if (!declaration) throw Error(`missing source function ${name}`);
  return declaration.getText(ast).replace(/^export\s+/, '');
};
let functions = `${extract('proxyTokenClient.ts', 'getTokenExpiration')}\n${extract('proxyTokenExpiry.ts', 'isTokenExpired')}`;
if (process.env.SAFE_CEX_PROXY_ORACLE_NEGATIVE_CONTROL === 'expiration') {
  const target = 'return decoded.exp * 1000;';
  if (!functions.includes(target)) throw Error('expiration negative-control target vanished');
  functions = functions.replace(target, 'return 0;');
}
const compiled = ts.transpileModule(functions, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
let decoded, now;
const codec = process.argv.includes('--codec');
const jwt = codec ? require(process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH, '../jsonwebtoken')
  : '../../../node_modules/jsonwebtoken') : { decode: () => decoded };
const oracle = new Function('jwt', 'virtualClock', `${compiled}\nreturn { getTokenExpiration, isTokenExpired };`)(
  jwt, { getCurrentTimeMs: () => now },
);
const values = [
  'null', 'true', 'false', '0', '-0', '2', '-2', '1e400', '-1e400',
  '""', '" "', '"-0"', '["-0"]', '"2"', '"0x10"', '"0X10"', '"0b10"', '"0o10"',
  '"+0x10"', '"0x"', '"0x200000000000011"', '"0x0000000000000010"',
  '"0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"', '"0b102"', '"Infinity"', '"+Infinity"', '"-Infinity"',
  '"inf"', '"infinity"', '"NaN"', '"1e400"', '"+2"', '".5"', '"2."', '"2e+2"',
  '"+"', '"2e"', '"2_0"', '"\\uFEFF2\\uFEFF"', '"\\u00852"',
  '[]', '[2]', '["0x10"]', '[null]', '[true]', '[false]', '[[]]', '[[2]]', '[2,3]',
  '{}', '{"valueOf":"ignored"}', '{"toString":null}', '[{"toString":null}]',
];
const payloads = [...(codec ? ['[]','null','false','true','0','""','"hello"','"{\\"exp\\":100}"'] : []), '{}', ...values.map(value => `{"exp":${value}}`)];
const header = Buffer.from('{"alg":"HS256"}').toString('base64url');
const cases = [];

const tokens = payloads.flatMap(payload => {
 const prefix = `${header}.${Buffer.from(payload).toString('base64url')}`;
 return codec ? ['signature','bad!','signature=','signature\n',''].map(signature=>({payload,token:`${prefix}.${signature}`}))
   : [{payload, token:`${prefix}.signature`}];
});
if(codec) {
 tokens.push({payload:'bad-header', token:`eA.${Buffer.from('{"exp":100}').toString('base64url')}.signature`});
 tokens.push({payload:'padded-header', token:`${header}=.${Buffer.from('{"exp":100}').toString('base64url')}.signature`});
 const raw=Buffer.concat([Buffer.from('"'),Buffer.from([255]),Buffer.from('"')]);
 tokens.push({payload:'lossy-utf8-string', token:`${header}.${raw.toString('base64url')}.signature`});
}
for (const {payload,token} of tokens) {
  if(!codec) decoded = JSON.parse(payload);
  const expiration = oracle.getTokenExpiration(token);
  for (now of [-1, 0, 1, 999, 1000, 2000, 16000, 16001, 100000]) {
    cases.push({ token, payload, now, expiration: String(expiration), negativeZero: Object.is(expiration, -0), expired: oracle.isTokenExpired(token) });
  }
}
console.log(JSON.stringify({ cases }));
