const fs=require('node:fs'),path=require('node:path');
const {ts,root}=require('./source.cjs');
module.exports=function loadModule(relative,names,imports,transformSource){
 const file=path.join(root,relative);
 let source=fs.readFileSync(file,'utf8')+'\nexport {'+names.join(',')+'};';
 if(transformSource)source=transformSource(source);
 const code=ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText;
 const module={exports:{}};
 new Function('require','module','exports',code)(name=>{if(!(name in imports))throw Error('Unexpected dependency '+name+' in '+file);return imports[name];},module,module.exports);
 return module.exports;
};
