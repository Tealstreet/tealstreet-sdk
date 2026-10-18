import { readFileSync } from 'node:fs';
import { STATUS_CODES } from 'node:http';
import ts from 'typescript';

const source = readFileSync(new URL('../../../../packages/safe-cex/src/utils/axiosErrorEnhancer.ts', import.meta.url), 'utf8');
const ast = ts.createSourceFile('axiosErrorEnhancer.ts', source, ts.ScriptTarget.Latest, true);
if (ast.parseDiagnostics.length) throw new Error('maintained axios enhancer failed to parse');
const body = ast.statements.filter(node => !ts.isImportDeclaration(node)).map(node => node.getText(ast)).join('\n');
const compiled = ts.transpileModule(body, {compilerOptions: {module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
const moduleExports = {};
new Function('exports', compiled)(moduleExports);

export function sourceFactoryErrorMessage(exchange, status, data, url) {
  const error = Object.assign(new Error(`Request failed with status code ${status}`), {
    config: {method: 'get', url},
    response: {status, statusText: STATUS_CODES[status], data},
  });
  return moduleExports.enhanceAxiosError(error, exchange).message;
}
