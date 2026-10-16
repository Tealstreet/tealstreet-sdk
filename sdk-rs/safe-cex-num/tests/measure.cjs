const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
const directory = process.argv[2];
if (!directory) throw new Error('output directory required');
fs.mkdirSync(directory, { recursive: true });
for (const [mode, flag] of [['standard', '--use-std-math-pow'], ['legacy', '--no-use-std-math-pow']]) {
  const result = spawnSync(process.execPath, [flag, path.join(__dirname, 'oracle.cjs')], { maxBuffer: 128 * 1024 * 1024 });
  if (result.status !== 0) throw new Error(String(result.stderr));
  const configuration = `node-${process.version}-${process.platform}-${process.arch}-${mode}`;
  fs.writeFileSync(path.join(directory, configuration + '.json'), result.stdout);
}
