// Preserve runtime numeric identity before JSON can collapse it.
// Bit captures record architecture and Node major; only NaN bits canonicalize.
function captureHeader(){return {captureArchitecture:process.arch,captureNodeMajor:Number(process.versions.node.split('.')[0])};}
function preserveSpecialNumbers(value){
 if(typeof value==='number'){
  if(!Number.isFinite(value))return {$number:Number.isNaN(value)?'NaN':value>0?'Infinity':'-Infinity'};
  if(Object.is(value,-0))return {$number:'-0'};
 }
 if(Array.isArray(value))return value.map(preserveSpecialNumbers);
 if(value!==null&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([key,v])=>[key,preserveSpecialNumbers(v)]));
 return value;
}
function numberBits(value){
 if(Number.isNaN(value))return '7ff8000000000000';
 const bytes=Buffer.alloc(8);bytes.writeDoubleBE(value);return bytes.readBigUInt64BE().toString(16).padStart(16,'0');
}
module.exports={captureHeader,preserveSpecialNumbers,numberBits};
