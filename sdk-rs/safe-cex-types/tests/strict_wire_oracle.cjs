const fs = require('node:fs');
const path = require('node:path');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const source = ts.createSourceFile('types.ts', fs.readFileSync(path.resolve(__dirname, '../../../packages/safe-cex/src/types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
const names = new Set(['OrderStatus','OrderType','OrderSide','OrderTimeInForce','OrderStopTriggerType','OrderRejectReason','PositionSide','PositionMode','MarginType','Timeframe','ExchangeName','DataProvenance','IncomeCategory','RequestOutcome','MessageHandlerStatsType']);
const result = {};
const declarations = new Map(source.statements.filter(n=>n.name).map(n=>[n.name.text,n]));
for (const node of source.statements) {
 if (!node.name || !names.has(node.name.text)) continue;
 if (ts.isEnumDeclaration(node)) result[node.name.text] = node.members.map(member => {if (!member.initializer || !ts.isStringLiteral(member.initializer)) throw Error('non-string enum');return member.initializer.text;});
 else if (ts.isTypeAliasDeclaration(node)) result[node.name.text] = (ts.isUnionTypeNode(node.type) ? node.type.types : [node.type]).map(type => {if (!ts.isLiteralTypeNode(type) || !ts.isStringLiteral(type.literal)) throw Error('non-string union');return type.literal.text;});
}
for (const name of names) if (!result[name]) throw Error('Missing live source union '+name);
function member(type, key) {
 const node = typeof type === 'string' ? declarations.get(type) : type;
 const members = node.members || (node.type && node.type.members);
 const found = members && members.find(n=>n.name && n.name.getText(source).replace(/['"]/g,'')===key);
 if (!found || !found.type) throw Error('Missing source property '+key);
 return found.type;
}
function literals(type) {
 if (ts.isUnionTypeNode(type)) return type.types.flatMap(literals);
 if (ts.isLiteralTypeNode(type) && ts.isStringLiteral(type.literal)) return [type.literal.text];
 throw Error('Expected closed literal union: '+type.getText(source));
}
for (const name of ['SecondsTimeframe','ExtendedTimeframe']) result[name]=literals(declarations.get(name).type);
for (const [rust,root,keys] of [
 ['ExchangeAccountConfigBybitSiteId','ExchangeAccountConfig',['bybitSiteId']],
 ['ExchangeAccountConfigHealthStatus','ExchangeAccountConfig',['healthStatus']],
 ['ExchangeOptionsBybitSiteId','ExchangeOptions',['bybitSiteId']],
 ['ExchangeOptionsExtraHyrotraderMode','ExchangeOptions',['extra','hyrotraderMode']],
 ['ExchangeOptionsExtraAsterdexApiVersion','ExchangeOptions',['extra','asterdexApiVersion']],
 ['ExchangeOptionsExtraLighterBuilderFeeMode','ExchangeOptions',['extra','lighterBuilderFeeMode']],
]) { let type=root;for(const key of keys) type=member(type,key);result[rust]=literals(type); }
const external=member('ExchangeOptions','externalPublicData');
for(let i=0;i<2;i++) result['ExchangeOptionsExternalPublicDataVariant'+i+'Type']=literals(member(external.types[i],'type'));
process.stdout.write(JSON.stringify(result));
