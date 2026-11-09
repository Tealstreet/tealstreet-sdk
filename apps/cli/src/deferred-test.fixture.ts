import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import vm from 'node:vm';
import ts from 'typescript';
export function deferred() {
  let resolve = () => {};
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
export function parsedSource(file: string) {
  return ts.createSourceFile(file, readFileSync(join(import.meta.dirname, file), 'utf8'), ts.ScriptTarget.Latest, true);
}
export function sourceFunction(file: string, name: string): string {
  const source = parsedSource(file);
  const node = source.statements.find(
    (node): node is ts.FunctionDeclaration => ts.isFunctionDeclaration(node) && node.name?.text === name,
  );
  if (!node) throw new Error(`Missing function ${file}:${name}`);
  return node.getText(source).replace(/^export /, '');
}
export function evaluate<T>(text: string, context: Record<string, unknown>): T {
  return vm.runInNewContext(
    ts.transpileModule(text, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText,
    context,
  );
}
export async function sessionFixture() {
  const config = await import('./config'),
    storage = await import('./storage-context');
  const login = (userId: string) => {
    const next = config.loadConfig();
    next.auth = { userId, accessToken: 'synthetic-fixture' };
    config.saveConfig(next);
  };
  login('A');
  return { config, storage, login };
}
