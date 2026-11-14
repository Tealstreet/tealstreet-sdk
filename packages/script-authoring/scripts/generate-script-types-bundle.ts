/** Write only the shared script-authoring bundle; no NX tooling is required. */
import { writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { generateScriptTypesBundle } from './render-script-types-bundle';

const output = resolve(__dirname, '../src/script-types-bundle.ts');
writeFileSync(output, generateScriptTypesBundle());
console.log(`Generated ${output}`);
