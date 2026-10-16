import fs from 'node:fs';
import {createRequire} from 'node:module';
const require = createRequire(import.meta.url);
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
// These source modules have no runtime imports; use CI's isolated compiler, never a local tsx install.
export async function loadStandalone(path) {
  const source = fs.readFileSync(path, 'utf8');
  const code = ts.transpileModule(source, {compilerOptions:{target:ts.ScriptTarget.ES2022, module:ts.ModuleKind.ESNext}}).outputText;
  return import('data:text/javascript;base64,' + Buffer.from(code).toString('base64'));
}
