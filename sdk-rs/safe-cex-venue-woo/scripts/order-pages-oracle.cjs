const fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/woo/woo.exchange.ts');
const ast = ts.createSourceFile(file, fs.readFileSync(file, 'utf8'), ts.ScriptTarget.Latest, true);
const cls = ast.statements.find(ts.isClassDeclaration);
const member = cls.members.find(n => n.name?.getText(ast) === 'fetchOrdersRecursively');
assert.ok(member, 'PAGES actual source member');
const source = member.getText(ast);
function worker(text) {
  const context = {};
  vm.runInNewContext(ts.transpileModule(`class Witness {${text}}; globalThis.Witness=Witness;`, {
    compilerOptions: {target: ts.ScriptTarget.ES2022},
  }).outputText, context);
  return new context.Witness();
}
function pack(value) {
  if (value === undefined) return {$undefined: true};
  if (typeof value === 'number') {
    const bytes = Buffer.alloc(8); bytes.writeDoubleBE(value);
    return {$bits: Number.isNaN(value) ? '7ff8000000000000' : bytes.toString('hex')};
  }
  if (Array.isArray(value)) return Array.from(value, pack);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k,v]) => [k,pack(v)]));
  return value;
}
// Expected requests and outputs are independent of the captured loop bounds.
const specs = [
  ['direct',1, ['a','b']], ['nested',1,['a','b']], ['empty',1,[]],
  ['two',2,['a','b','c','d']], ['three',3,['a','b','c','d','e','f']],
  ['disposed',1,[]], ['dispose-later',2,['a','b']], ['failure',1,[]],
  ['meta-message',1,[]], ['default-message',1,[]], ['failure-later',2,['a','b']],
  ['reject-later',2,null], ['logger-throw',1,null], ['missing-rows',1,null],
  ['missing-meta',1,null], ['nested-array',1,null], ['fraction',2,['a','b','c','d']],
  ['nan-page',1,['a','b']], ['mutate-earlier',2,['a','b','c','d']],
  ['noniterable-first',1,null], ['noniterable-tail',2,null], ['false-nested',1,['a','b']],
];
async function capture(text) {
  const cases = [];
  for (const [label] of specs) {
    const subject = worker(text), calls = [], logs = [], arrays = [];
    const network = new TypeError('network failure'), logger = new TypeError('logger failure');
    subject.isDisposed = label === 'disposed';
    subject.emitAndLogError = message => { logs.push(pack(message)); if (label === 'logger-throw') throw logger; };
    subject.xhr = {get: async (url, config) => {
      calls.push(pack({url,params:config.params}));
      const ordinal = calls.length;
      assert.ok(ordinal <= 3, 'PAGES bounded source calls');
      if (label === 'reject-later' && ordinal === 2) throw network;
      if (label === 'dispose-later' && ordinal === 2) subject.isDisposed = true;
      if (label === 'mutate-earlier' && ordinal === 2) arrays[0].splice(0);
      const rows = ['a','b','c','d','e','f'].slice((ordinal-1)*2, ordinal*2).map(id => ({id}));
      arrays.push(rows);
      const multi = ['two','dispose-later','failure-later','reject-later','fraction','mutate-earlier','noniterable-first','noniterable-tail'].includes(label);
      const total = label === 'three' ? 6 : multi ? 4 : 2;
      let data = {success:true,rows,meta:{total,records_per_page:2}};
      if (label === 'empty') { data.rows=[]; data.meta=null; }
      if (['failure','meta-message','default-message','logger-throw'].includes(label) || label === 'failure-later' && ordinal===2) {
        data = {success:false,message:label==='meta-message'||label==='default-message'?'':'top',meta:{message:label==='default-message'?0:'nested'}};
      }
      if (label === 'nested') data={success:true,data,rows:[{id:'wrong'}]};
      if (label === 'false-nested') data.data=false;
      if (label === 'nested-array') data.data=[];
      if (label === 'missing-rows') delete data.rows;
      if (label === 'missing-meta') delete data.meta;
      if (label === 'noniterable-first' || label === 'noniterable-tail' && ordinal===2) data.rows={length:2};
      return {data};
    }};
    const page = label==='fraction'?1.5:label==='nan-page'?NaN:1;
    let outcome;
    try {
      const rows = await subject.fetchOrdersRecursively('/order-test',page);
      outcome = {rows:pack(rows),alias:arrays.includes(rows)};
    } catch(error) {
      outcome={error:{name:error.name,message:error.message},networkIdentity:error===network,loggerIdentity:error===logger};
    }
    cases.push({label,page:pack(page),calls,logs,outcome});
  }
  return cases;
}
function validate(cases) {
  assert.equal(cases.length,22,'PAGES source total');
  let calls=0, outputs=0, reports=0;
  const errors={
    'reject-later':'network failure','logger-throw':'logger failure',
    'missing-rows':"Cannot read properties of undefined (reading 'length')",
    'missing-meta':"Cannot read properties of undefined (reading 'total')",
    'nested-array':"Cannot read properties of undefined (reading 'length')",
    'noniterable-first':'data.rows is not iterable',
    'noniterable-tail':'(intermediate value) is not iterable',
  };
  for(let i=0;i<22;i++) {
    const [label,count,ids]=specs[i], c=cases[i];
    assert.equal(c.label,label,'PAGES source identity');
    assert.equal(c.calls.length,count,'PAGES request cardinality '+label); calls+=count;
    for(let j=0;j<count;j++) assert.deepEqual(c.calls[j],pack({url:'/order-test',params:{status:'INCOMPLETE',page:label==='fraction'?1.5+j:label==='nan-page'?NaN:1+j}}),'PAGES exact request '+label);
    const message=label==='meta-message'?'nested':label==='default-message'?'Failed to fetch orders':'top';
    const logCount=['failure','meta-message','default-message','failure-later','logger-throw'].includes(label)?1:0;
    assert.deepEqual(c.logs,logCount?[message]:[],'PAGES message precedence '+label); reports+=logCount;
    if(ids!==null) {
      assert.equal(c.outcome.rows.length,ids.length,'PAGES output cardinality '+label);
      assert.deepEqual(c.outcome.rows,ids.map(id=>({id})),'PAGES output order '+label); outputs+=ids.length;
      assert.equal(c.outcome.alias,['direct','nested','nan-page','false-nested'].includes(label),'PAGES output alias '+label);
    } else {
      assert.deepEqual(c.outcome.error,{name:'TypeError',message:errors[label]},'PAGES error phase '+label);
      assert.equal(c.outcome.networkIdentity,label==='reject-later','PAGES network identity');
      assert.equal(c.outcome.loggerIdentity,label==='logger-throw','PAGES logger identity');
    }
  }
  assert.equal(calls,31,'PAGES total requests');
  assert.equal(outputs,30,'PAGES total outputs');
  assert.equal(reports,5,'PAGES total reports');
}
async function main() {
  const cases=await capture(source); validate(cases);
  if(process.argv.includes('--controls')) {
    const controls=[
      ['disposal','let { data } = await', 'if (this.isDisposed) return []; let { data } = await','PAGES request cardinality'],
      ['recursion','data.meta.total > data.meta.records_per_page * page','false','PAGES request cardinality'],
      ['concat','[...data.rows, ...(await this.fetchOrdersRecursively(endpoint, page + 1))]','[...(await this.fetchOrdersRecursively(endpoint, page + 1)), ...data.rows]','PAGES output order'],
      ['message','(data as any).message || (data as any).meta?.message','(data as any).meta?.message || (data as any).message','PAGES message precedence'],
    ];
    for(const [name,from,to,assertion] of controls) {
      assert.equal(source.split(from).length,2,'PAGES control admission '+name);
      let failure;
      try {validate(await capture(source.replace(from,to)));} catch(error) {failure=error;}
      assert.ok(failure?.message.includes(assertion),'PAGES named control '+name+': '+failure?.message);
      console.error(name+': '+assertion);
    }
  }
  process.stdout.write(JSON.stringify({capture:{architecture:process.arch,node:process.version,nan:'canonical-only',scope:'actual method; fake xhr, disposal and reporter'},cases}));
}
main().catch(error=>{console.error(error);process.exitCode=1;});
