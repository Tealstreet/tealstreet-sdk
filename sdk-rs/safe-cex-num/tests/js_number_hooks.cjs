const kinds = ['absent', 'object', 'undefined', 'null', 'false', '-0', 'string', 'bigint', 'symbol', 'throw'];
function val(k) { switch(k) {
  case 'object': return {}; case 'undefined': return undefined; case 'null': return null;
  case 'false': return false; case '-0': return -0; case 'string': return '\ufeff0x10\ufeff';
  case 'bigint': return 123456789012345678901234567890n; case 'symbol': return Symbol();
  case 'throw': throw new TypeError('hook failed');
}}
const rows = [];
for (const exotic of [false, true]) for (const first of kinds) for (const second of kinds) for (const unary of [false, true]) {
  const calls = [], o = Object.create(null);
  if (exotic) o[Symbol.toPrimitive] = () => { calls.push('exotic'); return val(first); };
  if (first !== 'absent') o.valueOf = () => {calls.push('valueOf'); return val(first);};
  if (second !== 'absent') o.toString = () => {calls.push('toString'); return val(second);};
  let expected;
  try {const n = unary ? +o : Number(o); expected = {ok: Number.isNaN(n) ? 'NaN' : Object.is(n,-0) ? '-0' : String(n)};}
  catch(e) { expected = {error:{name:e.name,message:e.message}}; }
  const numberCalls = [...calls]; calls.length = 0;
  let bigintExpected;
  try { bigintExpected = {ok:String(BigInt(o))}; }
  catch(e) { bigintExpected = {error:{name:e.name,message:e.message}}; }
  rows.push({exotic,first,second,unary,calls:numberCalls,expected,bigintCalls:[...calls],bigintExpected});
}
process.stdout.write(JSON.stringify(rows));
