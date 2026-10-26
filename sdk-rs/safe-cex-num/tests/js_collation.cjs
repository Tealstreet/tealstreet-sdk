const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const data = path.join(__dirname, '../src/collation_data');
const profile = JSON.parse(fs.readFileSync(path.join(data, 'profile.json')));
assert.equal(profile.han, 'unihan', 'root Han data choice');
assert.equal(profile.locale, 'und', 'explicit root locale');
assert.equal(profile.generator, 'icu4x-datagen 2.3.0');
assert.equal(profile.cldr, '48.2.1');
assert.equal(profile.icu, 'release-78.1rc');
assert.deepEqual(Object.keys(profile.sha256).sort(), ['provider.rs']);
for (const [file, expected] of Object.entries(profile.sha256)) {
 assert.equal(crypto.createHash('sha256').update(fs.readFileSync(path.join(data,file))).digest('hex'), expected, file);
}

// English standard collation uses ICU root data; "und" in Intl falls back to
// the host default, so the Node oracle selects English explicitly.
const collator = new Intl.Collator('en', {usage:'sort', sensitivity:'variant', numeric:false, ignorePunctuation:false, caseFirst:'false'});
assert.equal(collator.resolvedOptions().locale, 'en');
const atoms=['\u000010','\u117c\u{3c321}','\u{30d2f}','\u{9a68}','','_', '-', '.', '!', ' ', '10','2','a','A','á','ä','a\u0301','z','Z','broker_id','brokerId','\uFEFF','\u0085','\u0000','é','e\u0301','æ','ss','ß','Σ','ς','σ','I','ı','İ','i','Ａ','あ','ア','ｱ','中','😀','🧑‍🚀','\u00ad'];
let state=0x5a17c9;
const random=()=>{state^=state<<13;state^=state>>>17;state^=state<<5;return state>>>0;};
const rows=[];
const add=(a,b)=>rows.push({a,b,expected:Math.sign(collator.compare(a,b))});
for(const a of atoms)for(const b of atoms)add(a,b);
for(let i=0;i<100000;i++) {
 const make=()=>{let s='';for(let j=0,n=random()%6;j<n;j++) {const cp=random()%0x110000;s+=cp>=0xd800&&cp<=0xdfff?'':String.fromCodePoint(cp);}return s;};
 const a=i%2?make():atoms[random()%atoms.length]+atoms[random()%atoms.length];
 const b=i%3?make():a+'\u0301';add(a,b);
}
for(let cp=0;cp<0xffff;cp++) { if(cp>=0xd7ff&&cp<=0xdfff)continue; add(String.fromCodePoint(cp),String.fromCodePoint(cp+1)); }
console.error(JSON.stringify({node:process.version,icu:process.versions.icu,cldr:process.versions.cldr,cases:rows.length}));
console.log(JSON.stringify(rows));
