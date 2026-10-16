const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const runtime = require('./oracle-runtime.cjs');
const root = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/woo');
const read = name => fs.readFileSync(path.join(root, name), 'utf8');
const transpile = source => ts.transpileModule(source, {
  compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022},
}).outputText;
function moduleExports(source) {
  const exports = {};
  vm.runInNewContext(transpile(source), {exports, require: runtime});
  return exports;
}
const ast = ts.createSourceFile('woo.exchange.ts', read('woo.exchange.ts'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const names = ['fetchTransactionHistoryRows', 'fetchLegacyTransactionHistoryRows', 'getTransactionHistorySymbolCandidates', 'parseTransactionRowsFromResponse'];
const members = names.map(name => {
  const member = cls.members.find(n => n.name?.getText(ast) === name);
  assert.ok(member, name);
  return member.getText(ast);
}).join('\n');
const types = ts.createSourceFile('woo.types.ts', read('woo.types.ts'), ts.ScriptTarget.Latest, true);
const endpoint = types.statements.find(n => ts.isVariableStatement(n) && n.declarationList.declarations.some(d => d.name.getText(types) === 'ENDPOINTS'));
assert.ok(endpoint, 'HISTORY source ENDPOINTS');
const bindings = {...moduleExports(read('woo.utils.ts')), ...moduleExports(read('woo.legacy-fills.ts')), ...moduleExports(endpoint.getText(types))};
function worker(source) {
  const context = {...bindings};
  vm.runInNewContext(transpile(`class Witness {${source}}\nglobalThis.Witness=Witness;`), context);
  return new context.Witness();
}
const symbols = [undefined, '', 'btcusdt', 'ethusdt'];
const scenarios = [
  ['empty', [], []], ['primary-first', ['rows'], []],
  ['primary-second', ['empty', 'rows'], []],
  ['primary-unqualified', ['empty','empty','empty','empty','rows'], []],
  ['legacy-first', [], ['rows']], ['legacy-last', [], ['empty','empty','empty','empty','rows']],
  ['primary-error', ['error'], []], ['primary-error-legacy-rows', ['error'], ['rows']],
  ['both-errors', ['error'], ['error']], ['legacy-error', [], ['error']],
  ['primary-null', ['null'], []], ['primary-error-null', ['error','null'], []],
  ['primary-error-legacy-null', ['error'], ['error','null']],
  ['mutated-options', ['mutate'], ['rows']],
  ['primary-false', ['false'], []], ['primary-zero', ['zero'], []],
  ['primary-string', ['string'], []], ['primary-undefined', ['undefined'], []],
];
function pack(value) {return value === undefined ? {$undefined:true} : value;}
async function capture(source) {
  const cases=[];
  for (let symbolIndex=0;symbolIndex<4;symbolIndex++) {
    for (const [scenario, primary, legacy] of scenarios) {
      const subject=worker(source), calls=[], errors=[], arrays=[];
      const opts={limit:37,fromMilliseconds:0,toMilliseconds:1700000000123};
      if(symbols[symbolIndex]!==undefined)opts.symbol=symbols[symbolIndex];
      const inputOpts={...opts};
      const attempts={primary:0,legacy:0};
      subject.xhr={get:async(url,config)=>{
        const phase=url===bindings.ENDPOINTS.TRANSACTION_HISTORY?'primary':'legacy';
        assert.equal(url,phase==='primary'?bindings.ENDPOINTS.TRANSACTION_HISTORY:bindings.ENDPOINTS.CLIENT_TRADES,'HISTORY route');
        const ordinal=attempts[phase]++;
        calls.push({phase,ordinal,url,params:config.params});
        assert.ok(calls.length<=10,'HISTORY bounded attempts');
        const action=(phase==='primary'?primary:legacy)[ordinal]||'empty';
        if(action==='mutate'){opts.symbol='SOLUSDT';opts.limit=99;}
        if(action==='error'){
          const error={phase,ordinal,message:`${phase}-error-${ordinal}`};errors.push(error);throw error;
        }
        if(['null','false','zero','string','undefined'].includes(action)) {
          const error={null:null,false:false,zero:0,string:'',undefined:undefined}[action];
          throw error;
        }
        const rows=action==='rows'?[{phase,id:'first',qty:3.25},null,{phase,id:'last',qty:7.5}]:[];
        arrays.push(rows);
        return {data:{data:{rows},rows:[{id:'wrong-container'}]}};
      }};
      let outcome;
      try {
        const rows=await subject.fetchTransactionHistoryRows(opts);
        outcome={rows,alias:arrays.includes(rows)};
      } catch(error) {
        outcome={threw:true,error:pack(error),errorIdentity:errors.includes(error)};
      }
      cases.push({symbolIndex,scenario,opts:inputOpts,finalOpts:{...opts},calls,outcome});
    }
  }
  assert.equal(cases.length,72,'HISTORY source total');
  return cases;
}
function validate(cases) {
  const counts=[10,1,2,5,6,10,10,6,10,10,10,10,10,6,10,10,10,10];
  let total=0;
  for(let index=0;index<72;index++) {
    const c=cases[index];
    assert.equal(c.calls.length,index<36?1:counts[index%18],'HISTORY per-case attempt count '+index);
    total+=c.calls.length;
    if(index<36)assert.equal(c.calls[0].phase,'primary','HISTORY no-symbol primary only');
    for(let callIndex=0;callIndex<c.calls.length;callIndex++) {
      const call=c.calls[callIndex];
      const phase=callIndex<5?'primary':'legacy';
      const ordinal=callIndex%5;
      assert.equal(call.phase,phase,'HISTORY ordered phase');
      assert.equal(call.ordinal,ordinal,'HISTORY ordered ordinal');
      assert.equal(call.url,phase==='primary'?'/v3/trade/transactionHistory':'/v1/client/trades','HISTORY exact route');
      const asset=phase==='legacy'&&c.scenario==='mutated-options'?'SOL':c.symbolIndex===2?'BTC':'ETH';
      const candidates=[`PERP_${asset}_USDT`,`${asset}_USDT_PERP`,`${asset}-USDT-PERP`,`${asset}USDT`];
      const symbol=index<36?undefined:candidates[ordinal];
      const limit=c.scenario==='mutated-options'&&callIndex>0?99:37;
      const expected=phase==='primary'?{limit,startTime:0,endTime:1700000000123}:{size:limit,start_t:0,end_t:1700000000123};
      if(symbol!==undefined)expected.symbol=symbol;
      assert.deepEqual(Object.keys(call.params).sort(),Object.keys(expected).sort(),'HISTORY phase parameter keys');
      for(const key of Object.keys(expected))assert.equal(call.params[key],expected[key],'HISTORY parameter '+key);
    }
  }
  assert.equal(total,328,'HISTORY source request total');
  const at=(scenario,symbolIndex=2)=>cases.find(c=>c.scenario===scenario&&c.symbolIndex===symbolIndex);
  for(const name of ['null','false','zero','string','undefined']) {
    const c=at('primary-'+name,0);
    assert.equal(c.outcome.threw,true,'HISTORY direct falsy rejection '+name);
    assert.deepEqual(c.outcome.error,pack({null:null,false:false,zero:0,string:'',undefined:undefined}[name]),'HISTORY direct falsy value '+name);
    assert.equal(at('primary-'+name).outcome.rows.length,0,'HISTORY loop suppresses falsy '+name);
  }
  assert.equal(at('primary-error-null').outcome.rows.length,0,'HISTORY primary falsy overwrite');
  assert.equal(at('primary-error-null').outcome.alias,false,'HISTORY primary falsy fresh empty');
  assert.equal(at('both-errors').outcome.error.phase,'legacy','HISTORY legacy error wins');
  assert.equal(at('both-errors').outcome.errorIdentity,true,'HISTORY legacy error identity');
  for(const scenario of ['primary-error','primary-error-legacy-null']) {
    assert.equal(at(scenario).outcome.error.phase,'primary','HISTORY retained primary error '+scenario);
    assert.equal(at(scenario).outcome.errorIdentity,true,'HISTORY primary error identity '+scenario);
  }
  for(const scenario of ['primary-first','primary-second','primary-unqualified','legacy-first','legacy-last','primary-error-legacy-rows']) {
    assert.equal(at(scenario).outcome.rows.length,3,'HISTORY successful row count '+scenario);
    assert.equal(at(scenario).outcome.alias,true,'HISTORY successful array identity '+scenario);
  }
  assert.equal(at('mutated-options').calls[5].params.symbol,'PERP_SOL_USDT','HISTORY legacy recomputes candidates');
  assert.equal(at('empty').outcome.alias,false,'HISTORY fresh final empty');
}
async function main() {
  const cases=await capture(members);validate(cases);
  if(process.argv.includes('--controls')) {
    for(const [label,before,after,assertion] of [
      ['direct','if (!opts.symbol) {','if (false) {','HISTORY per-case attempt count'],
      ['legacy','const legacyRows = await this.fetchLegacyTransactionHistoryRows(opts);','const legacyRows = [];','HISTORY per-case attempt count'],
      ['early-error','const legacyRows = await this.fetchLegacyTransactionHistoryRows(opts);','if (lastError) throw lastError;\nconst legacyRows = await this.fetchLegacyTransactionHistoryRows(opts);','HISTORY per-case attempt count'],
    ]) {
      assert.equal(members.split(before).length,2,'HISTORY mutation admission '+label);
      const mutated=await capture(members.replace(before,after));
      assert.throws(()=>validate(mutated),error=>error.code==='ERR_ASSERTION'&&error.message.includes(assertion),'HISTORY source control '+label);
      console.error('HISTORY source control assertion-red: '+label);
    }
  }
  process.stdout.write(JSON.stringify({architecture:process.arch,nodeMajor:process.versions.node.split('.')[0],cases}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
