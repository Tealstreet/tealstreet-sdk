'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {spawnSync} = require('node:child_process');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const prior = spawnSync(process.execPath, [path.join(__dirname,'records.cjs')], {encoding:'utf8'});
assert.equal(prior.status,0,`GRVT AUDIT existing checker fixture: ${prior.stderr}`);
const original = JSON.parse(prior.stdout);
assert.equal(original.length,1782,'GRVT AUDIT original fixture floor');
const selected = [
  ['GrvtBuilderAuthorizationResult','message'],['GrvtBuilderCheckResult','maxFeeRate'],['GrvtBuilderInfo','builder'],
  ['GrvtSignature','r'],['OrderLeg','instrument'],['OrderPayload','subAccountId'],
  ['GrvtInstrument','instrument_id'],['GrvtInstrumentLite','i'],['GrvtTicker','instrument_id'],
  ['GrvtOrderBookLevel','p'],['GrvtOrderBook','instrument_id'],['GrvtTrade','trade_id'],
  ['GrvtKline','timestamp'],['GrvtPosition','sub_account_id'],['GrvtBalance','currency'],
  ['GrvtAccountSummary','sub_account_id'],['GrvtOrder','order_id'],['GrvtFill','fill_id'],
  ['GrvtAuthRequest','api_key'],['GrvtAuthResponse','session_token'],
  ['GrvtWebSocketSubscription','params.channel'],['GrvtWebSocketMessage','error.message'],
];
const captured = new Set(['GrvtBuilderCheckResult','GrvtBuilderInfo','GrvtSignature','OrderPayload','GrvtInstrumentLite','GrvtWebSocketSubscription','GrvtWebSocketMessage']);
const positive = ['scalaré','😀','\ufffd','\\ud800',' \t\ufeff'];
const rows = [], probes = [];
let completed = 0, deepCompleted = 0;
function replace(base,field,value){const result=JSON.parse(base);let node=result;const parts=field.split('.');for(const key of parts.slice(0,-1))node=node[key];node[parts.at(-1)]=value;return result;}
function read(value,field){for(const key of field.split('.'))value=value[key];return value;}
for(const [name,field] of selected){
  const base = original.filter(r=>r.name===name && r.label==='all-fields');
  assert.equal(base.length,1,'GRVT AUDIT exactly one source baseline');
  assert.equal(base[0].accepted,true,'GRVT AUDIT actual source baseline admitted');
  const positives = positive.map(value=>{
    const raw=JSON.stringify(replace(base[0].raw,field,value));
    assert.equal(read(JSON.parse(raw),field),value,'GRVT AUDIT source scalar value');
    return raw;
  });
  const negatives = [];
  for(const unit of [0xd800,0xdc00]){
    const raw=JSON.stringify(replace(base[0].raw,field,String.fromCharCode(unit)));
    const actual=read(JSON.parse(raw),field);
    assert.equal(typeof actual,'string','GRVT AUDIT source returned string type');
    assert.equal(actual.length,1,'GRVT AUDIT source returned unit length');
    assert.equal(actual.charCodeAt(0),unit,'GRVT AUDIT source exact retained code unit');
    probes.push(`const grvtAudit${probes.length}: ${name} = ${raw};`);
    negatives.push(raw);
  }
  if(captured.has(name)){
    let extension={};for(let i=0;i<140;i++)extension={next:extension};
    const extended=JSON.parse(positives[0]);extended.x=extension;
    const observed=JSON.parse(JSON.stringify(extended));
    assert.equal(read(observed,field),'scalaré','GRVT AUDIT source deep-extension sibling');
    let count=0;for(let node=observed.x;Object.hasOwn(node,'next');node=node.next)count++;
    assert.equal(count,140,'GRVT AUDIT source unknown-extension depth');
    probes.push(`const grvtDeepBase${deepCompleted}: ${name} = ${base[0].raw}; const grvtDeep${deepCompleted}: ${name} = Object.assign(grvtDeepBase${deepCompleted}, {x: {} as unknown});`);
    deepCompleted++;
  }
  rows.push({name,field,positives,negatives,captured:captured.has(name)});
  completed++;
}
const file=path.resolve(__dirname,'../../../packages/safe-cex/src/exchanges/grvt/grvt.types.ts');
let source=fs.readFileSync(file,'utf8');
for(const [owner,names] of [['grvt.signing.ts',['OrderLeg','GrvtSignature','OrderPayload']],['grvt.builder.ts',['GrvtBuilderCheckResult','GrvtBuilderInfo','GrvtBuilderAuthorizationResult']]]){
  const filename=path.join(path.dirname(file),owner);const tree=ts.createSourceFile(filename,fs.readFileSync(filename,'utf8'),ts.ScriptTarget.Latest,true);
  for(const name of names){const declaration=tree.statements.find(n=>ts.isInterfaceDeclaration(n)&&n.name.text===name);assert(declaration,'GRVT AUDIT actual interface');source+='\n'+declaration.getText(tree);}
}
source+='\n'+probes.join('\n');
const options={strict:true,types:[],skipLibCheck:true,target:ts.ScriptTarget.ES2022};
const host=ts.createCompilerHost(options),readFile=host.readFile;
host.readFile=name=>name===file?source:readFile(name);
const program=ts.createProgram([file],options,host);
assert.equal(program.getSemanticDiagnostics().length,0,'GRVT AUDIT actual source surrogate and structural typing');
assert.equal(completed,22,'GRVT AUDIT source completed record count');
assert.equal(deepCompleted,7,'GRVT AUDIT source completed captured record count');
process.stdout.write(JSON.stringify({rows,completed,deepCompleted}));
