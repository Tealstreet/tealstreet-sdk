// A members-only census cannot establish inherited-field provenance.
module.exports=function refuseInterfaceHeritage(ts,source) {
 for(const node of source.statements) {
  if(ts.isInterfaceDeclaration(node)&&node.heritageClauses?.length) {
   throw Error('Unsupported interface heritage: '+node.name.text+' in '+source.fileName);
  }
 }
};
