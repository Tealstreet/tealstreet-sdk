const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = fs.readFileSync(path.resolve(__dirname, '../../../packages/http-over-websocket/src/axiosHttpOverWebsocketAdapter.ts'), 'utf8');
const start = source.indexOf('      // Handle GRVT custom auth properties');
const end = source.indexOf('      // Check if this is a GRVT request', start);
if (start < 0 || end < 0) throw new Error('GRVT consumed config source anchors missing');
const code = ts.transpileModule(`(function(config) { const newHeaders = { ...config.headers }; ${source.slice(start, end)} return newHeaders; })`, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
const project = eval(code);
let cases = [
  {},
  {grvtCookie:'rm=true; gravity=session-42',grvtAccountId:'42'},
  {grvtCookie:'',grvtAccountId:''},
  {grvtCookie:null,grvtAccountId:false},
  {grvtCookie:0,grvtAccountId:0},
  {grvtCookie:'gravity=replace',grvtAccountId:'0'},
  {grvtCookie:[],grvtAccountId:{}},
];
if (process.argv.includes('--native')) {
  cases = [undefined, NaN, Infinity, -0, {missing:undefined, nan:NaN, infinity:Infinity}]
    .map(value => ({grvtCookie:value, grvtAccountId:value}));
}
process.stdout.write(JSON.stringify(cases.map((extra, index) => {
  const config = { headers: {Cookie:'original', 'X-Grvt-Account-Id':'original-account', 'X-Ts-Grvt-Session':'session-42'}, ...extra };
  return {index, extra, headers:config.headers, expected:project(config)};
})));
