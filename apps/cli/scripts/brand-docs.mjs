#!/usr/bin/env node
// Generate a whitelabel copy of the CLI user docs.
//
//   node scripts/brand-docs.mjs <brand> <out-dir>
//
// Source of truth stays apps/documentation/cli (Docusaurus, Tealstreet-branded).
// For another brand this script copies that tree, drops Docusaurus-only syntax
// (front matter, :::admonitions) so GitHub renders it, rewrites every
// brand-bound token (binary name, config dir, release repo, env prefix,
// routing flags, hosts), applies whole-file overrides from
// docs-brands/<brand>/ for pages whose content differs (install channels),
// writes a README.md index in sidebar order, and then REFUSES if any file
// still mentions the source brand. The release workflow pushes the result
// to the brand's public release repo on every <brand>-cli-v* tag.
//
// Node builtins only — this runs in CI before any workspace build.
import { createRequire } from 'node:module';
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, existsSync, rmSync } from 'node:fs';
import { join, dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const cliDir = resolve(here, '..');
const docsSrc = resolve(cliDir, '../documentation/cli');
const sidebarFile = resolve(cliDir, '../documentation/sidebarsCli.js');

const BRANDS = {
  disrupt: {
    name: 'Disrupt',
    bin: 'disrupt',
    configDir: '.disrupt',
    envPrefix: 'DISRUPT',
    releaseRepo: 'DisruptTrade/cli',
    webHost: 'app.disrupt.trade',
    // The source brand, spelled so this file never has to repeat it elsewhere.
    source: { name: 'Tealstreet', bin: 'tealstreet', configDir: '.tealstreet', envPrefix: 'TEALSTREET' },
  },
};

const [brandId, outArg] = process.argv.slice(2);
const brand = BRANDS[brandId];
if (!brand || !outArg) {
  console.error('Usage: brand-docs.mjs <disrupt> <out-dir>');
  process.exit(2);
}
const outDir = resolve(outArg);
const overridesDir = join(cliDir, 'docs-brands', brandId);
const src = brand.source;

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else if (name.endsWith('.md') && name !== 'CLAUDE.md') out.push(p);
  }
  return out;
}

function stripFrontMatter(text) {
  const m = text.match(/^---\n([\s\S]*?)\n---\n/);
  if (!m) return { title: null, body: text };
  const title = (m[1].match(/^title:\s*(.+)$/m) || [])[1] || null;
  return { title, body: text.slice(m[0].length).replace(/^\n+/, '') };
}

// :::tip Title\n...\n::: -> a blockquote with a bold lead-in.
function convertAdmonitions(text) {
  return text.replace(/^:::(\w+)\s*(.*)\n([\s\S]*?)^:::\s*$/gm, (_, kind, title, body) => {
    const head = title.trim() || kind[0].toUpperCase() + kind.slice(1);
    const quoted = body
      .trim()
      .split('\n')
      .map((l) => (l ? `> ${l}` : '>'))
      .join('\n');
    return `> **${head}**\n>\n${quoted}`;
  });
}

const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

function rebrand(text) {
  const S = src;
  const rules = [
    // Pages of the Tealstreet docs site that do not exist for this brand.
    [/\[([^\]]*?)MCP connector\]\(\/docs\/[^)]+\)/g, (_, pre) => `${pre}MCP connector`],
    // Install channels the brand does not have; the install pages are overridden
    // wholesale, this handles the one inline block in getting-started.
    [
      /```bash\n# macOS \/ Linux one-liner[\s\S]*?npm install -g @tealstreet\/cli\n```/,
      `Download the binary for your platform from the\n[latest release](https://github.com/${brand.releaseRepo}/releases/latest),\nmake it executable and put it on your \`PATH\`:\n\n\`\`\`bash\ncurl -fsSL https://github.com/${brand.releaseRepo}/releases/latest/download/${brand.bin}-darwin-arm64 -o /usr/local/bin/${brand.bin}\nchmod +x /usr/local/bin/${brand.bin}\n\`\`\``,
    ],
    // Release repo and hosts.
    [new RegExp(`https://github\\.com/${S.name}/cli`, 'g'), `https://github.com/${brand.releaseRepo}`],
    [new RegExp(`\`${S.name}/cli\``, 'g'), `\`${brand.releaseRepo}\``],
    [new RegExp(`${S.name}/cli\\b`, 'g'), brand.releaseRepo],
    [/https:\/\/(app|preview|v3)\.tealstreet\.io/g, `https://${brand.webHost}`],
    [/\b(app|preview|v3)\.tealstreet\.io\b/g, brand.webHost],
    // Routing flags: the brand's help shows the neutral spelling.
    [new RegExp(`--${S.bin}-proxy`, 'g'), '--proxy'],
    [new RegExp(`--${S.bin}-tunnel`, 'g'), '--tunnel'],
    [new RegExp(`--${S.bin}-ws-route`, 'g'), '--ws-route'],
    [new RegExp(`"Use ${S.name} IPs"`, 'g'), 'the proxy routing option'],
    // Env overrides, asset names, config dir, binary.
    [new RegExp(`${S.envPrefix}_`, 'g'), `${brand.envPrefix}_`],
    [new RegExp(`${S.bin}-(darwin|linux|windows|backup|accounts)`, 'g'), `${brand.bin}-$1`],
    [new RegExp(`${S.bin}\\.exe`, 'g'), `${brand.bin}.exe`],
    [new RegExp(escapeRe(S.configDir), 'g'), brand.configDir],
    [new RegExp(`${S.name}'s`, 'g'), `${brand.name}'s`],
    [new RegExp(S.name, 'g'), brand.name],
    [new RegExp(`\\b${S.bin}\\b`, 'g'), brand.bin],
  ];
  return rules.reduce((t, [re, rep]) => t.replace(re, rep), text);
}

function sidebarOrder() {
  const require = createRequire(import.meta.url);
  const sidebars = require(sidebarFile);
  const out = [];
  const visit = (items, group) => {
    for (const it of items) {
      if (typeof it === 'string') out.push({ id: it, group });
      else if (it.type === 'category') visit(it.items, it.label);
    }
  };
  visit(Object.values(sidebars)[0], null);
  return out;
}

if (existsSync(outDir)) rmSync(outDir, { recursive: true });
mkdirSync(outDir, { recursive: true });

const titles = new Map();
for (const file of walk(docsSrc)) {
  const rel = relative(docsSrc, file);
  const override = join(overridesDir, rel);
  let text = existsSync(override) ? readFileSync(override, 'utf8') : readFileSync(file, 'utf8');
  const { title, body } = stripFrontMatter(text);
  text = rebrand(convertAdmonitions(body));
  const h1 = (body.match(/^# (.+)$/m) || [])[1];
  titles.set(rel.replace(/\.md$/, ''), rebrand(title || h1 || rel));
  const dest = join(outDir, rel);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, text);
}

// README.md: the sidebar, as a linked index.
const lines = [`# ${brand.name} CLI documentation`, ''];
let currentGroup = null;
for (const { id, group } of sidebarOrder()) {
  if (!titles.has(id)) continue;
  if (group !== currentGroup) {
    lines.push('', `## ${group ?? 'Start here'}`, '');
    currentGroup = group;
  }
  lines.push(`- [${titles.get(id)}](./${id}.md)`);
}
lines.push('');
writeFileSync(join(outDir, 'README.md'), lines.join('\n'));

// The gate: nothing of the source brand may survive, in any case.
const leak = new RegExp(src.name, 'i');
const leaks = walk(outDir).filter((f) => leak.test(readFileSync(f, 'utf8')));
if (leaks.length) {
  console.error(`brand-docs: source brand still mentioned in:\n  ${leaks.map((f) => relative(outDir, f)).join('\n  ')}`);
  process.exit(1);
}
console.log(`brand-docs: wrote ${titles.size + 1} files to ${outDir}`);
