const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH||'typescript');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../../');
const file = ts.createSourceFile('hyperliquid.types.ts', fs.readFileSync(path.join(root, 'packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.types.ts'), 'utf8'), ts.ScriptTarget.Latest, true);
require('./interface-heritage.cjs')(ts,file);
const defs = new Map(), claims = {}, out = [];
const snake = s => s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
const pascal = s => s.replace(/(^|[_:-])(\w)/g, (_,a,b)=>b.toUpperCase());
const quoted = JSON.stringify;
let owner;
function claim(name) { (claims[owner] ??= []).push(name); }
function emit(name, text) { if(defs.has(name)) return; defs.set(name,text); claim(name); }
function shape(name, members) {
 if(members.length===1 && ts.isIndexSignatureDeclaration(members[0])) {
  // These reviewed roots use the shared raw-JSON own-data carriers. Keep this
  // routing explicit so source regeneration cannot restore lexical BTree order.
  const snapshots={AllMids:ts.SyntaxKind.StringKeyword,UserRateLimit:ts.SyntaxKind.AnyKeyword,OrderStatus:ts.SyntaxKind.AnyKeyword};
  if(Object.hasOwn(snapshots,name)) {
   require('node:assert/strict').equal(members[0].type.kind,snapshots[name],'review core snapshot value shape before changing its carrier');
   require('node:assert/strict').equal(members[0].parameters[0].type.kind,ts.SyntaxKind.StringKeyword,'review core snapshot key shape');
   return emit(name, `pub type ${name} = crate::json_models::${name};`);
  }
  return emit(name, `pub type ${name} = std::collections::BTreeMap<String, ${type(members[0].type, name+'Value')}>;`);
 }
 const fields = members.filter(ts.isPropertySignature).map(m=>{
  const n=m.name.text, rust = n.length===1 && n.toUpperCase()===n ? snake(n)+"_upper" : snake(n); let t=type(m.type, name+pascal(n));
  const optional=!!m.questionToken, nullable=t.startsWith('Option<');
  if(optional) t=`Option<${t}>`;
  const ingress=optional?', default, deserialize_with = "deserialize_present", skip_serializing_if = "Option::is_none"':nullable?', deserialize_with = "deserialize_required_nullable"':'';
  return `    #[serde(rename = ${quoted(n)}${ingress})]\n    pub ${['type','ref','self'].includes(rust)?'r#'+rust:rust}: ${t},`;
 });
 emit(name, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct ${name} {\n${fields.join('\n')}\n}`);
}
function type(n, name) {
 if(!n) throw Error(name);
 if(ts.isParenthesizedTypeNode(n)) return type(n.type,name);
 if(ts.isTypeReferenceNode(n)) { const id=n.typeName.getText(file); return id==='Array'?`Vec<${type(n.typeArguments[0],name+'Item')}>`:id; }
 if(ts.isArrayTypeNode(n)) return `Vec<${type(n.elementType,name+'Item')}>`;
 if(ts.isTupleTypeNode(n)) return `(${n.elements.map((e,i)=>type(e,name+'Item'+i)).join(', ')},)`;
 if(ts.isTypeLiteralNode(n)) { shape(name,n.members);return name; }
 if(ts.isUnionTypeNode(n)) {
  const nonnull=n.types.filter(t=>t.getText(file)!=='null');
  if(nonnull.length<n.types.length) return `Option<${type(nonnull.length===1?nonnull[0]:ts.factory.createUnionTypeNode(nonnull),name)}>`;
  if(n.types.every(t=>ts.isLiteralTypeNode(t)&&ts.isStringLiteral(t.literal))) {
   emit(name, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub enum ${name} {\n${n.types.map(t=>`    #[serde(rename = ${quoted(t.literal.text)})]\n    ${pascal(t.literal.text)},`).join('\n')}\n}`);return name;
  }
  const variants=n.types.map((t,i)=>`    Case${i}(${type(t,name+'Case'+i)}),`);
  emit(name,`#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\n#[serde(untagged)]\npub enum ${name} {\n${variants.join('\n')}\n}`);return name;
 }
 if(ts.isLiteralTypeNode(n) && ts.isStringLiteral(n.literal)) { emit(name, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub enum ${name} {\n    #[serde(rename = ${quoted(n.literal.text)})]\n    ${pascal(n.literal.text)},\n}`); return name; }
 if(ts.isLiteralTypeNode(n)) return 'f64';
 if(ts.isIntersectionTypeNode(n)) {
  require('node:assert/strict').equal(name,'WsUserEvent','review a new native intersection before routing it');
  require('node:assert/strict').equal(n.getText(file).replace(/\s/g,''),'(WsFill[]|WsUserFunding|WsLiquidation|WsNonUserCancel[])&{user:string}','review core event declaration before sharing native variants');
  return 'crate::spot_user_event::WsUserEvent';
 }
 const k=n.kind;
 if(k===ts.SyntaxKind.StringKeyword) return 'String';
 if(k===ts.SyntaxKind.NumberKeyword) return 'f64';
 if(k===ts.SyntaxKind.BooleanKeyword) return 'bool';
 if(k===ts.SyntaxKind.AnyKeyword||k===ts.SyntaxKind.UnknownKeyword) return 'serde_json::Value';
 throw Error('Unhandled '+ts.SyntaxKind[k]+' '+name);
}
const NOT_MODELED = require('./not-modeled-types.cjs');
for(const statement of file.statements) {
 if((ts.isInterfaceDeclaration(statement)||ts.isTypeAliasDeclaration(statement))&&NOT_MODELED.has(statement.name.text)) continue;
 if(ts.isInterfaceDeclaration(statement)) {
  owner=statement.name.text;
  // TypeScript declaration merging (UserFills and WsUserFunding).
  const peers=file.statements.filter(x=>ts.isInterfaceDeclaration(x)&&x.name.text===owner);
  const members=new Map(); for(const p of peers) for(const m of p.members) members.set(m.name?.text??m.getText(file),m);
  if(!defs.has(owner)) shape(owner,[...members.values()]);
 } else if(ts.isTypeAliasDeclaration(statement)) {
  owner=statement.name.text;const result=type(statement.type,owner);if(result!==owner) emit(owner,`pub type ${owner} = ${result};`);
 } else if(ts.isVariableStatement(statement)) {
  for(const d of statement.declarationList.declarations) {
   owner=d.name.text; const e=d.initializer;
   if(ts.isObjectLiteralExpression(e)) {
    const pairs=e.properties.map(p=>`(${quoted(p.name.text)}, ${quoted(p.initializer.text)})`);
    emit(owner,`pub const ${owner}: &[(&str, &str)] = &[${pairs.join(', ')}];`);
   } else if(ts.isArrayLiteralExpression(e)) {
    const vals=e.elements.map(e=>{const table=file.statements.filter(ts.isVariableStatement).flatMap(s=>[...s.declarationList.declarations]).find(d=>d.name.text===e.expression.text).initializer;return quoted(table.properties.find(p=>p.name.text===e.name.text).initializer.text)});
    emit(owner,`pub const ${owner}: &[&str] = &[${vals.join(', ')}];`);
   }
  }
 }
}
const strictDefs=[...defs.values()].map(text => {
 if(text.startsWith('#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub struct')) {
  const name=text.match(/pub struct (\w+)/)[1], body=text.slice(text.indexOf('{'));
  const publicBody=body.replace(/    #\[serde\(.*?\)\]\n/g,'');
  return `#[derive(Debug, Clone, PartialEq)]\npub struct ${name} ${publicBody}\n#[derive(Serialize, Deserialize)]\n#[serde(remote = "${name}")]\nstruct ${name}Wire ${body}\nstrict_record!(${name}, ${name}Wire);`;
 }
 if(text.includes('#[serde(untagged)]')) {
  const name=text.match(/pub enum (\w+)/)[1], body=text.slice(text.indexOf('{'));
  return `#[derive(Debug, Clone, PartialEq)]\npub enum ${name} ${body}\n#[derive(Serialize, Deserialize)]\n#[serde(remote = "${name}", untagged)]\nenum ${name}Wire ${body}\nstrict_numeric_union!(${name}, ${name}Wire);`;
 }
 if(!text.startsWith('#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]\npub enum')) return text;
 const name=text.match(/pub enum (\w+)/)[1];
 if(text.includes('Case0(')) return text;
 const body=text.slice(text.indexOf('{'));
 const publicBody=body.replace(/    #\[serde\(rename = .*?\)\]\n/g,'');
 return `#[derive(Debug, Clone, PartialEq)]\npub enum ${name} ${publicBody}\n#[derive(Serialize, Deserialize)]\n#[serde(remote = "${name}")]\nenum ${name}Wire ${body}\nstrict_literal!(${name}, ${name}Wire);`;
});
const wireImpl=require('./wire-impl.cjs');
const expanded=strictDefs.map(text=>text.replace(/strict_(literal|record|numeric_union)!\((\w+), (\w+)\);/g,(_,kind,name,wire)=>wireImpl(kind,name,wire)));
// Explicit implementations stay visible to parity indexing; only the field
// check is an expression macro. Aliases use their existing recursive backend.
const registrations = [...defs.entries()].flatMap(([name,text]) => {
 const record = text.includes(`pub struct ${name} {`), enumeration = text.includes(`pub enum ${name} {`);
 if (!record && !enumeration) return [];
 const members = record
  ? [...text.matchAll(/^    pub ([\w#]+):/gm)].map(m=>m[1]).join(', ')
  : [...text.matchAll(/^    (\w+)(?:\((.+)\))?,/gm)].map(m=>m[1]+(m[2]?'(value)':'')).join(', ');
 return [`impl safe_cex_num::numeric_ingress::JsNumberOutput for ${name} {
 fn check_number_output(&self) {
  safe_cex_num::js_number_output!(self, ${record?'struct':'enum'} { ${members} });
 }
}`];
});
const fieldIngress = `
fn deserialize_present<'de,D:serde::Deserializer<'de>,T:Deserialize<'de>>(decoder:D)->Result<Option<T>,D::Error> {
 T::deserialize(decoder).map(Some)
}
`+([...defs.values()].some(text=>text.includes('deserialize_required_nullable'))?`fn deserialize_required_nullable<'de,D:serde::Deserializer<'de>,T:Deserialize<'de>>(decoder:D)->Result<Option<T>,D::Error> {
 Option::<T>::deserialize(decoder)
}
`:'');
const source='//! Typed wire models from hyperliquid.types.ts; JS numbers remain binary64.\nuse serde::{Serialize, Deserialize};\n\n'+fieldIngress+'\n'+expanded.join('\n\n')+'\n\n'+registrations.join('\n\n')+'\n';
const claimsText=JSON.stringify(claims,null,2)+'\n';
if(process.argv.includes('--check')) {
 const {spawnSync}=require('node:child_process');
 const formatted=spawnSync('rustfmt',['--edition','2024','--emit','stdout'],{input:source,encoding:'utf8'});
 require('node:assert/strict').equal(formatted.status,0,'rustfmt required for generated source check');
 require('node:assert/strict').equal(formatted.stdout,fs.readFileSync(path.join(__dirname,'../src/types.rs'),'utf8'),'strict wire source regeneration drift');
 require('node:assert/strict').equal(claimsText,fs.readFileSync(path.join(__dirname,'../tests/fixtures/type-claims.json'),'utf8'),'type claim regeneration drift');
} else {
 fs.writeFileSync(path.join(__dirname,'../src/types.rs'),source);
 fs.writeFileSync(path.join(__dirname,'../tests/fixtures/type-claims.json'),claimsText);
}
