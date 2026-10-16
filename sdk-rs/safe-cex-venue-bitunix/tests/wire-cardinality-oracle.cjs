// Separately labelled supplement; original wire oracle remains byte-identical.
const fs = require('node:fs');
const path = require('node:path');
let original;
const captureProcess = { stdout: {write: text => { original = JSON.parse(text); }} };
const extension = `
const cardinalityRows = [base, {...base, symbol:'second-row', positionId:'second-id', qty:'2.375'},
 {...base, symbol:'third-row', positionId:'third-id', qty:'3.875'}];
const supplement = [[], cardinalityRows].map((value,i)=>({
 label:'cardinality supplement '+i, raw:JSON.stringify(value), accepted:true,
 native:moduleRecord.exports.capture(value).map(row=>Object.fromEntries(Object.entries(row).map(([key,value])=>[key,observe(value)]))),
}));
emit(supplement);
`;
new Function('require','__dirname','process','emit',
 fs.readFileSync(path.join(__dirname,'wire-oracle.cjs'),'utf8')+extension)(
 require,__dirname,captureProcess,supplement=>process.stdout.write(JSON.stringify({...original,supplement})));
