const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const file = path.resolve(__dirname, '../../../packages/lighter-sdk/src/types/responses.ts');
let source = fs.readFileSync(file, 'utf8');
const mutation = process.env.LIGHTER_SDK_RESPONSES_MUTATION;
if (mutation === 'inherited-number') source = source.replace(/(interface Result(?:<[^>]+>)?)(\s*\{)/, '$1 extends ExtraNumeric$2') + '\ninterface ExtraNumeric { hiddenNumeric: number; }\n';
if (mutation === 'result-code-number') source = source.replace('code: ResultCode;', 'code: number;');
if (mutation === 'key-number') source = source.replace('publicKey: string;', 'publicKey: number;');
if (mutation === 'last-used-required') source = source.replace('lastUsedAt?: number;', 'lastUsedAt: number;');
if (mutation === 'deposit-amount-number') source = source.replace('minDeposit: string;', 'minDeposit: number;');
if (mutation === 'services-scalar') source = source.replace('affectedServices: string[];', 'affectedServices: string;');
if (mutation === 'funding-time-string') source = source.replace('nextFundingTime: number;', 'nextFundingTime: string;');
const dictionaryKind = kind => kind === 'Record<string, string>';
const stringKind = kind => kind === 'string' || kind === 'string[]' || dictionaryKind(kind);
const numericKind = kind => ['number','ResultCode'].includes(kind);
if (mutation === 'api-keys-scalar') source = source.replace('apiKeys: ApiKeyInfo[];', 'apiKeys: ApiKeyInfo;');
if (mutation === 'rate-limits-scalar') source = source.replace('rateLimits: RateLimit[];', 'rateLimits: RateLimit;');
if (mutation === 'error-fields-number') source = source.replace('fieldErrors?: Record<string, string>;', 'fieldErrors?: Record<string, number>;');
if (mutation === 'fee-tiers-scalar') source = source.replace('makerTiers: FeeTier[];', 'makerTiers: FeeTier;');
if (mutation === 'fund-balances-number') source = source.replace('balances: Record<string, string>;', 'balances: Record<string, number>;');
if (mutation === 'health-status-extra') source = source.replace('"healthy" | "degraded" | "unhealthy"', '"healthy" | "degraded" | "unhealthy" | "unknown"');
if (mutation === 'connection-number') source = source.replace('database?: "connected" | "disconnected";', 'database?: number;');
if (mutation === 'component-status-extra') source = source.replace('"operational" | "degraded" | "down"', '"operational" | "degraded" | "down" | "unknown"');
if (mutation === 'kyc-status-extra') source = source.replace('"none" | "pending" | "verified" | "rejected"', '"none" | "pending" | "verified" | "rejected" | "unknown"');
if (mutation === 'withdrawal-fee-number') source = source.replace('fee: string;', 'fee: number;');
if (mutation === 'deposit-required-string') source = source.replace('requiredConfirmations: number;', 'requiredConfirmations: string;');
if (mutation === 'maintenance-required') source = source.replace('scheduledMaintenance?: MaintenanceWindow[];', 'scheduledMaintenance: MaintenanceWindow[];');
const tree = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true);
const schemas = {}, complete = {}, numeric = {}, strings = {}, nestedNumeric = {}, nestedStrings = {}, dictionaries = {}, unions = {};
const nestedKind = kind => ['ApiKeyInfo[]','RateLimit[]','FeeTier[]','ComponentStatus[]','MaintenanceWindow[]'].includes(kind);
const nestedFixture = (kind, tag) => {
  const childName=kind.slice(0,-2);
  const child=Object.fromEntries(Object.entries(complete[childName]).map(([k,v])=>[k,unions[childName][k]?v:typeof v==='string'?`${tag}:${v}`:v]));
  const other=Object.fromEntries(Object.entries(child).map(([k,v])=>[k,unions[childName][k]?unions[childName][k][1]:Array.isArray(v)?v.map(item=>`${item}:second`):typeof v==='string'?`${v}:second`:typeof v==='number'?v+1:!v]));
  return [child,other,child,child];
};
const bits = n => { const b=Buffer.alloc(8);b.writeDoubleBE(n);return b.toString('hex'); };
for (const name of ["Result", "NextNonce", "ApiKeyInfo", "TxHash", "ServerTime", "RateLimit", "DepositAddress", "TransferHistoryItem", "MaintenanceWindow", "FeeTier", "FundingRate", "AccountApiKeys", "ExchangeInfo", "ErrorResponse", "FeeSchedule", "InsuranceFund", "HealthResponse", "ComponentStatus", "AccountInfoResponse", "WithdrawalHistoryItem", "DepositHistoryItem", "SystemStatus"]) {
  const node = tree.statements.find(n => ts.isInterfaceDeclaration(n) && n.name.text === name);
  assert.ok(node, name);
  assert.ok(!node.heritageClauses?.length, `${name} unsupported interface heritage`);
  schemas[name] = node.members.map(f=>({name:f.name.text,type:f.type.getText(tree),optional:!!f.questionToken}));
  unions[name] = Object.fromEntries(node.members.filter(f=>ts.isUnionTypeNode(f.type)).map(f=>[f.name.text,f.type.types.map(t=>{assert.ok(ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal),`${name}.${f.name.text} string literal`);return t.literal.text;})]));
  complete[name] = Object.fromEntries(schemas[name].map(f=>[f.name,unions[name][f.name]?unions[name][f.name][0]:dictionaryKind(f.type)?Object.fromEntries(Object.entries(JSON.parse('{"z":"first","10":"ten","2":"two","a":"last","__proto__":"own"}')).map(([k,v])=>[k,`${name}.${f.name}:${v}`])):nestedKind(f.type)?nestedFixture(f.type,`${name}.${f.name}`):numericKind(f.type)?16777217.25:f.type==='boolean'?true:f.type==='string[]'?[`${name}.${f.name}.b`,`${name}.${f.name}.a`,`${name}.${f.name}.b`,`${name}.${f.name}.b`]:`${name}.${f.name}`]));
  numeric[name] = schemas[name].filter(f=>numericKind(f.type)).flatMap(f => ['-0','-0.0','-0e0','0','1e3','1e400','-1e400','-1e-4000','16777217.25','9007199254740993'].map(raw=> {
    const value={...complete[name],[f.name]:'__NUMBER__'};
    const encoded=JSON.stringify(value).replace('"__NUMBER__"',raw);
    return {field:f.name,raw,encoded,bits:bits(JSON.parse(encoded)[f.name])};
  }));
  strings[name] = ['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y'].map(text=>{
    const value={...complete[name]};
    for (const f of schemas[name].filter(f=>stringKind(f.type))) value[f.name]=dictionaryKind(f.type)?{key:text}:f.type==='string[]'?[text,text]:text;
    const encoded=JSON.stringify(value), parsed=JSON.parse(encoded);
    for (const f of schemas[name].filter(f=>stringKind(f.type))) for (const item of (dictionaryKind(f.type)?Object.values(parsed[f.name]):f.type==='string[]'?parsed[f.name]:[parsed[f.name]])) assert.equal(item,text);
    return {encoded,units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),representable:text.isWellFormed()};
  });
  nestedNumeric[name] = schemas[name].filter(f=>nestedKind(f.type)).flatMap(f=>numeric[f.type.slice(0,-2)].map(w=>({
    field:f.name,childField:w.field,raw:w.raw,bits:w.bits,
    encoded:JSON.stringify({...complete[name],[f.name]:['__NESTED__']}).replace('"__NESTED__"',w.encoded)
  })));
  nestedStrings[name] = schemas[name].filter(f=>nestedKind(f.type)).flatMap(f=>strings[f.type.slice(0,-2)].map(w=>({
    field:f.name,units:w.units,representable:w.representable,
    encoded:JSON.stringify({...complete[name],[f.name]:['__NESTED__']}).replace('"__NESTED__"',w.encoded)
  })));

  dictionaries[name] = schemas[name].filter(f=>dictionaryKind(f.type)).flatMap(f => {
    const raws=['{}','{"z":"first","10":"ten","2":"two","a":"last","01":"zero-one","4294967294":"index","4294967295":"not-index","__proto__":"own","constructor":"own-function-name","z":"replacement"}'];
    const samples=raws.map(raw=>({raw,representable:true,units:null,axis:"order"}));
    for(const text of ['','plain','é','👩','\ufeff','\u0085','\ud800','\udfff','x\ud800y']) for(const axis of ['key','value']) {
      const raw=JSON.stringify(axis==='key'?{[text]:'value'}:{key:text});
      const parsed=JSON.parse(raw);
      assert.equal(axis==='key'?Object.keys(parsed)[0]:parsed.key,text);
      samples.push({raw,representable:text.isWellFormed(),units:Array.from({length:text.length},(_,i)=>text.charCodeAt(i)),axis});
    }
    return samples.map(w=>({field:f.name,...w,keys:w.representable?Object.keys(JSON.parse(w.raw)):null,encoded:JSON.stringify({...complete[name],[f.name]:'__DICTIONARY__'}).replace('"__DICTIONARY__"',w.raw)}));
  });
}
process.stdout.write(JSON.stringify({schemas,complete,numeric,strings,nestedNumeric,nestedStrings,dictionaries,unions}));
