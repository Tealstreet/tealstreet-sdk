const assert=require('node:assert/strict');

// Runtime array values alone cannot establish the TypeScript indexed union.
function indexedLiterals(ts,source,node,parse){
 assert.ok(ts.isIndexedAccessTypeNode(node));
 assert.equal(node.indexType.kind,ts.SyntaxKind.NumberKeyword,'indexed literal union must retain number index');
 let object=node.objectType;
 while(ts.isParenthesizedTypeNode(object))object=object.type;
 assert.ok(ts.isTypeQueryNode(object),'indexed literal union must query a declared tuple');
 const name=object.exprName.getText(source);
 const declaration=source.statements.filter(ts.isVariableStatement).flatMap(n=>Array.from(n.declarationList.declarations)).find(d=>d.name.getText(source)===name);
 assert.ok(declaration&&!declaration.type,'literal tuple must not have an unclassified type annotation');
 const initializer=declaration.initializer;
 assert.ok(initializer&&ts.isAsExpression(initializer)&&ts.isTypeReferenceNode(initializer.type)&&initializer.type.typeName.getText(source)==='const','literal tuple must retain as const');
 assert.ok(ts.isArrayLiteralExpression(initializer.expression),'literal tuple must remain an array');
 const values=initializer.expression.elements.map(parse);
 return {name,values};
}
function indexedStringLiterals(ts,source,node){return indexedLiterals(ts,source,node,item=>{assert.ok(ts.isStringLiteral(item),'string literal tuple element');return item.text});}
function indexedNumberLiterals(ts,source,node){return indexedLiterals(ts,source,node,item=>{assert.ok(ts.isNumericLiteral(item),'number literal tuple element');return Number(item.text)});}
module.exports={indexedStringLiterals,indexedNumberLiterals};
