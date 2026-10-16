import {load} from './source.mjs';
const api=load('packages/safe-cex/src/exchanges/mexc/mexc.api.ts');
// The JSON texts are data, not source-level surrogate characters. Keeping the
// text AND JSON.stringify(output) double-escapes the witness for Rust ingress.
const inputs=[
 ['{"x":"\\ud800"}',false],['{"x":"\\udfff"}',false],['{"\\ud800":"x"}',false],
 ['"\\ud800"',false],['{"x":"\\ud800x"}',false],
 ['{"x":"hello"}',true],['{"x":"é中"}',true],['{"x":"\\ud83d\\ude03"}',true],
 ['{"x":"\\ufeff \\t\\n\\u0085"}',true],['"😀"',true],
];
process.stdout.write(JSON.stringify(inputs.map(([json,representable])=>{
 const value=JSON.parse(json);
 return {json,representable,tsAccepted:true,outputs:{
  serializeMexcParams:JSON.stringify(api.serializeMexcParams(value)),
  getQueryString:JSON.stringify(api.getQueryString({params:value,url:'/private?x=1'})),
  normalizeRequestData:JSON.stringify(api.normalizeRequestData(value)),
 }};
})));
