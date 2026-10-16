const assert = require('node:assert/strict');
const path = require('node:path');
assert(process.env.SAFE_CEX_TYPESCRIPT_PATH, 'SAFE_CEX_TYPESCRIPT_PATH required');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH);
const file = path.resolve(__dirname, '../../../packages/safe-cex/src/exchanges/apexomni/zklink/signer/types.ts');
const options = { strict: true, target: ts.ScriptTarget.ES2022 };
const host = ts.createCompilerHost(options);
const originalRead = host.readFile.bind(host);
host.readFile = name => {
  let source = originalRead(name);
  if (name === file && process.env.APEXOMNI_SIGNER_RECORD_MUTATION) {
    const from = "export interface TxEthSignature {";
    assert.equal(source.split(from).length, 2);
    source = source.replace(from, 'interface HeritageControl { inheritedRequired: string; }\nexport interface TxEthSignature extends HeritageControl {');
  }
  return source;
};
const program = ts.createProgram([file], options, host);
const errors = ts.getPreEmitDiagnostics(program).filter(d=>d.category===ts.DiagnosticCategory.Error && d.file?.fileName===file);
assert.equal(errors.length,0,errors.map(d=>ts.flattenDiagnosticMessageText(d.messageText,'\n')).join('\n'));
const checker = program.getTypeChecker();
const source = program.getSourceFile(file);
assert(source, 'actual signer source loaded');
const declaration = source.statements.find(n => ts.isInterfaceDeclaration(n) && n.name.text === 'TxEthSignature');
assert(declaration, 'actual TxEthSignature declaration');
const fields = checker.getPropertiesOfType(checker.getTypeAtLocation(declaration)).map(symbol => {
  const type = checker.getTypeOfSymbolAtLocation(symbol, symbol.valueDeclaration);
  return {name:symbol.name, optional:!!(symbol.flags & ts.SymbolFlags.Optional),
    kind:type.flags & ts.TypeFlags.String?'string':'union',
    literals:type.isUnion()?type.types.map(t=>{assert(t.flags & ts.TypeFlags.StringLiteral);return t.value;}):[]};
});
const kinds = fields.find(f=>f.name==='type').literals;
const signatures = ['', '0x00', 'signature-é-😀', 'EIP1271Signature', 'EthereumSignature'];
const samples = kinds.flatMap(type => signatures.map(signature => {
  const value = {type, signature};
  return {value, typeType:typeof value.type, signatureType:typeof value.signature, serialized:JSON.stringify(value)};
}));
const boundary = JSON.parse('{"type":"EthereumSignature","signature":"\\ud800"}');
assert.equal(boundary.signature.charCodeAt(0),0xd800);
console.log(JSON.stringify({boundary:{units:[boundary.signature.charCodeAt(0)],serialized:JSON.stringify(boundary)},header:{nodeMajor:Number(process.versions.node.split('.')[0]),nodeArch:process.arch,rustArch:process.env.SAFE_CEX_RUST_ARCH},fields,samples}));
