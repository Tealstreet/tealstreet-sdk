const fs=require('node:fs');
if(!process.env.SAFE_CEX_TYPESCRIPT_PATH)throw Error('isolated TypeScript compiler required');
const ts=require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const path=require('node:path').join(__dirname,'../../../packages/safe-cex/src/journal/position.types.ts');
const ast=ts.createSourceFile(path,fs.readFileSync(path,'utf8'),ts.ScriptTarget.Latest,true);
const aliases=Object.fromEntries(ast.statements.filter(ts.isTypeAliasDeclaration).map(n=>[n.name.text,n.type]));
function strings(node){return ts.isUnionTypeNode(node)?node.types.map(n=>n.literal.text):[];}
const record=aliases.PositionRecord;
const unions={};
for(const name of ['PositionCloseReason','PositionProvenance','PositionSourceAxis'])unions[name]=strings(aliases[name]);
// positionSide uses a PositionSide reference and one literal: do not stringify the reference.
unions.PositionRecordLeg=['long','short',...record.members.find(n=>n.name.text==='positionSide').type.types.filter(ts.isLiteralTypeNode).map(n=>n.literal.text)];
unions.PositionRecordState=strings(record.members.find(n=>n.name.text==='state').type);
const fields=record.members.map(n=>({name:n.name.text,optional:!!n.questionToken,type:n.type.getText(ast)}));
console.log(JSON.stringify({unions,fields}));
