const assert = require('node:assert/strict');
const fs = require('node:fs');
const units = s => Array.from({ length: s.length }, (_,i) => s.charCodeAt(i));
let buffer = '', count = 0;
function flush() { fs.writeSync(1,buffer); buffer = ''; }
function emit(text,needle,replacement) {
  buffer += JSON.stringify([units(text),units(needle),units(replacement),units(text.replace(needle,replacement)),units(text.replaceAll(needle,replacement))]) + '\n';
  count++;
  if (buffer.length > 32768) flush();
}
const texts = ['', 'a', 'abaaba', 'aaaa', '_a$&b', '{"_a$&b":1,"_$$":2}', '😀', 'a😀b😀', '\ud800', '\udfff', 'a\ud800b', '\ufeffa\u0085', '日本', 'e\u0301', '\0a\0'];
const needles = ['', 'a', 'aa', 'aba', '_a$&b', '_$$', '😀', '\ud83d', '\ude00', '\ud800', '\udfff', 'none', '\ufeff', '\0'];
const replacements = ['', '_', '$$', '$&', '$`', "$'", '$1', '$01', '$0', '$<name>', '$$$&', '$$&', '$`$&$\'$$', 'a$&b', '😀', '\ud800', '\udfff', '\ufeff'];
for (const text of texts) for (const needle of needles) for (const replacement of replacements) emit(text,needle,replacement);
let state = 0xd418511;
function random() { state ^= state << 13; state ^= state >>> 17; state ^= state << 5; return state >>> 0; }
const alphabet = [...'abc_$&`\'19', 0xd800,0xdc00,0xd83d,0xde00,0xfeff,0,0x85].map(x => typeof x === 'number' ? String.fromCharCode(x) : x);
function string(max) { let s='';const n=random()%max;for(let i=0;i<n;i++)s+=alphabet[random()%alphabet.length];return s; }
for (let i=0;i<30000;i++) {
  const text=string(18);
  const needle=i%2 ? text.slice(random()%(text.length+1),random()%(text.length+1)) : string(5);
  emit(text,needle,string(14));
}
const measurement=require('./fixtures/replace-private-measurement.json');
for (const row of measurement.rows) {
  assert.deepEqual(units(row.input[0].replaceAll(row.input[1],row.input[2])),row.node,'private-copy witness stays measured');
}
assert.notDeepEqual(measurement.rows[0].private,measurement.rows[0].node,'empty needle splits astral code units');
for (const row of measurement.rows.slice(1)) assert.deepEqual(row.private,row.node,'nonempty private-copy evidence agrees');
flush();
console.error(JSON.stringify({count,node:process.version}));
