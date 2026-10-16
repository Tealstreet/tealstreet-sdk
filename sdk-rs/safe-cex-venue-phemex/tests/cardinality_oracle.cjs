// Supplement the original corpus without editing its bytes or spawning another Node.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const originalPath = path.join(__dirname, 'types_oracle.cjs');
const original = fs.readFileSync(originalPath, 'utf8');
let baseline;
const localProcess = {env:process.env, version:process.version, versions:process.versions, arch:process.arch, platform:process.platform, stdout:{write:text=>{baseline=JSON.parse(text);}}};
const helpers = vm.runInThisContext(`(function(require,__dirname,process){${original}\nreturn {fixture,response,observe};})`,{filename:originalPath})(require,__dirname,localProcess);
assert.equal(baseline.cases.length,71,'source baseline case total');
assert.equal(baseline.envelopes.length,74,'source baseline envelope total');
const supplement = [0,2,3].map(count=>{
 const value=helpers.fixture(helpers.response);
 value.positions=Array.from({length:count},()=>helpers.fixture(helpers.response.positions[0]));
 const raw=JSON.stringify(value), parsed=JSON.parse(raw);
 return {raw, count, expected:parsed.positions.map(row=>helpers.observe({account:parsed.account,positions:[row]}))};
});
assert.equal(supplement.reduce((n,c)=>n+c.count,0),5,'source supplement position total');
process.stdout.write(JSON.stringify({...baseline,supplement}));
