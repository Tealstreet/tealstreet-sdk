const fs=require('node:fs'),path=require('node:path'),ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const {dependencies}=require('../scripts/accounts-source.cjs');
const root=path.resolve(__dirname,'../../..');
function load(file,names,extra={}){const source=fs.readFileSync(path.join(root,'packages/safe-cex/src/utils',file),'utf8').replace(/^import .*;$/mg,'').replace(/export /g,'');const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022}}).outputText;return new Function(...Object.keys(extra),code+'\nreturn {'+names.join(',')+'};')(...Object.values(extra));}
const helpers={...load('incomeCategoryFromType.ts',['incomeCategoryFromType']),...load('normalizeExecutionPositionSide.ts',['normalizeExecutionPositionSide'],{PositionSide:{Long:'long',Short:'short'}}),...load('createPositionId.ts',['createPositionId'],{PositionMode:{OneWay:'oneway'}}),...load('inverseObj.ts',['inverseObj']),...load('replaceUrlParams.ts',['replaceUrlParams']),...load('clone.ts',['isPlainObject','fastClone','clone']),...load('omitUndefined.ts',['omitUndefined'],{omitBy:dependencies('lodash/omitBy')})};
const rows=[];function add(op,args){try{rows.push({op,args,expected:helpers[op](...args)});}catch(error){rows.push({op,args,error:error.message,errorName:error.name});}}
for(const raw of [undefined,null,'REALIZED_PNL',' delivered_settelment ','DELIVERED_SETTELMENT','FUNDING_FEE','commission','COMMISSION_REBATE','TRANSFER','INSURANCE_CLEAR',12,true,[],{},{toString:'long'},{toString:null},{toString:12},'\uFEFFClose Long\uFEFF','\u0085long\u0085','sell','Open Short','long'])for(const op of ['incomeCategoryFromType','normalizeExecutionPositionSide'])add(op,[raw]);
for(const side of [undefined,'long','short'])for(const mode of [undefined,'oneway','hedged'])add('createPositionId',['BTCUSDT',side,mode]);
for(const object of [{a:'x',b:'x'},JSON.parse('{"2":"a","1":"b","x":"1","z":"__proto__"}'),{a:123,b:1e-7,c:'constructor'}])add('inverseObj',[object]);
for(const params of [{symbol:'BTCUSDT',count:0},{symbol:'$&$`$\'$$',count:2},{symbol:'',count:-1},{a:'{b}',b:'x'},{}])for(const url of [undefined,'','/{symbol}/{count}/{symbol}','/x/{a}/{b}','/{oops-{symbol}}','/{missing}','/{toString}/{constructor}/{__proto__}'])add('replaceUrlParams',[url,params]);
for(const object of [null,0,'text',[],{z:1,a:2},JSON.parse('{"__proto__":{"a":1},"nested":{"2":2,"1":1},"null":null}'),[{a:[null,{z:1}]}]])for(const op of ['isPlainObject','fastClone','clone'])add(op,[object]);
const omitted={x:undefined,n:null,y:1,__proto__:null};rows.push({op:'omitUndefined',args:[[['x',null],['n',null],['y',1]]],expected:helpers.omitUndefined(omitted)});
rows.push({op:'omitSpecial',args:[],expected:helpers.omitUndefined(JSON.parse('{"__proto__":1,"constructor":2,"prototype":3,"2":4,"1":5,"z":6}'))});
for(const value of [{toString:null},{toString:12},[{toString:false}],[[{toString:{}}]]]) {
 for(const op of ['incomeCategoryFromType','normalizeExecutionPositionSide'])add(op,[value]);
 add('inverseObj',[{x:value}]);add('replaceUrlParams',['/{symbol}',{symbol:value}]);
}
process.stdout.write(JSON.stringify(rows));
