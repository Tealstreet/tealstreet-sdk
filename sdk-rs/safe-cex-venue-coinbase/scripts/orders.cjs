const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm'),assert=require('node:assert/strict');
const {preserveSpecialNumbers}=require('./special-numbers.cjs');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH ?? 'typescript');
const root=path.resolve(__dirname,'../../..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');
function load(text, imports={}) {
 const exports={};
 vm.runInNewContext(ts.transpileModule(text,{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText,{exports,require:name=>{
  assert.ok(Object.hasOwn(imports,name),`unexpected oracle dependency ${name}`);return imports[name];
 }});return exports;
}
const parse=p=>ts.createSourceFile(p,read(p),ts.ScriptTarget.Latest,true);
const enums=load(parse('packages/safe-cex/src/types.ts').statements.filter(ts.isEnumDeclaration).map(s=>s.getText()).join('\n'));
const util=parse('packages/safe-cex/src/exchanges/coinbase/coinbase.utils.ts');
const norm=util.statements.find(s=>ts.isVariableStatement(s)&&s.declarationList.declarations.some(d=>d.name.getText(util)==='normalizeCfmContractSize'));
assert.ok(norm,'normalizeCfmContractSize exists');
const utils=load(norm.getText(util));
const math=load(read('packages/safe-cex/src/utils/safe-math.ts'),{'@tealstreet/math/src/isNotNumber':load(read('packages/math/src/isNotNumber.ts'))});
const orders=load(read('packages/safe-cex/src/exchanges/coinbase/coinbase.orders.ts'),{'../../types':enums,'../../utils/safe-math':math,'./coinbase.utils':utils});
const values=[null,true,false,{},[],1,0,-1,0.001,1e-20,1e20,'',' ','\uFEFF0.01\uFEFF','\u00850.01','0x10','0b11','0o10','12abc','Infinity','1e999','-0','+0','NaN','0.0100','1e-7','0x20000000000001'];
const sizes=values.map(value=>({value,result:utils.normalizeCfmContractSize(value)??null}));
const market={id:'BTC-USD',symbol:'BTC-USDC',base:'BTC',quote:'USD',active:true,precision:{amount:0.001,price:0.01},limits:{amount:{min:0.001,max:100000},leverage:{min:1,max:10}}};
const raw={order_id:'id',client_order_id:'client',product_id:'BTC-USD',side:'BUY',status:'OPEN',order_type:'LIMIT',created_time:'2026-01-01',filled_size:'0.3',average_filled_price:'12',extra_unknown:{keep:true},order_configuration:{limit_limit_gtc:{base_size:'1.5',limit_price:'10'}}};
const cases=[];
function add(name,overrides={},marketOverrides={}) {
 const order={...raw,...overrides}, m={...market,...marketOverrides};
 cases.push({name,order,market:m,result:preserveSpecialNumbers(orders.mapCoinbaseRestOrder(order,m,12345)??null)});
}
for(const key of ['market_market_ioc','market_market_fok','limit_limit_gtc','limit_limit_gtd','limit_limit_fok','sor_limit_ioc','stop_limit_stop_limit_gtc','stop_limit_stop_limit_gtd']) {
 for(const value of values) {
  add(`${key}: base ${JSON.stringify(value)}`,{order_configuration:{[key]:{base_size:value,quote_size:'12',limit_price:'5',stop_price:'6'}}});
 }
}
for(const value of [...values,undefined]) {
 add(`filled ${JSON.stringify(value)}`,{filled_size:value});
 add(`CFM ${JSON.stringify(value)}`,{}, {isExpiring:true,contractSize:typeof value==='number'?value:undefined});
 add(`quote ${JSON.stringify(value)}`,{order_configuration:{limit_limit_gtc:{quote_size:value,limit_price:'3'}}});
 add(`price ${JSON.stringify(value)}`,{order_configuration:{limit_limit_gtc:{quote_size:'10',limit_price:value}}});
}
for(const status of ['PENDING','OPEN','QUEUED','CANCEL_QUEUED','EDIT_QUEUED','FILLED','CANCELLED','EXPIRED','FAILED','UNKNOWN_ORDER_STATUS','constructor','__proto__','toString','valueOf','hasOwnProperty']) add(`status ${status}`,{status});
for(const side of ['BUY','SELL','UNKNOWN'])add(`side ${side}`,{side});
for(const filled_size of ['1.5','2','-1','invalid'])add(`fill edge ${filled_size}`,{filled_size});
for(const order_configuration of [undefined,null,{}, {market_market_ioc:{},limit_limit_gtc:{base_size:'5',limit_price:'4'}},{market_market_ioc:null,market_market_fok:{base_size:'2'}},{limit_limit_gtc:{base_size:'2',limit_price:'9'},sor_limit_ioc:{base_size:'4',limit_price:'1'}},{stop_limit_stop_limit_gtc:{base_size:'2',stop_price:'',limit_price:'9'}},{stop_limit_stop_limit_gtc:{base_size:'2',stop_price:'0',limit_price:'9'}}])add('configuration precedence',{order_configuration});
for(const size of [0.01,0.001,1,100])add(`contract multiplier ${size}`,{},{isExpiring:true,contractSize:size});
for(const base_size of ['1e308','1e-308','5e-324','1.5'])for(const filled_size of ['0','1e-308','5e-324','0.3'])for(const contractSize of [1,1e308,1e-308])add('numeric boundaries',{filled_size,order_configuration:{limit_limit_gtc:{base_size,limit_price:'-0'}}},{isExpiring:true,contractSize});
for(const quote_size of ['1e308','1e-308','5e-324'])for(const limit_price of ['1e308','1e-308','5e-324','1'])add('division boundaries',{filled_size:'0',order_configuration:{limit_limit_gtc:{quote_size,limit_price}}});
process.stdout.write(JSON.stringify({sizes,cases}));
