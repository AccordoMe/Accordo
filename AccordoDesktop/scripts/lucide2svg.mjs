// Writes lucide icons as standalone SVGs for assets/icons/ (white strokes, tinted at runtime).
// Reads the icon definitions from an installed lucide-react (the web app's node_modules).
//
//   node scripts/lucide2svg.mjs ../node_modules/lucide-react/dist/esm/icons assets/icons pencil gift …
//
// Some names are aliases (smile → face-slightly-smiling): those are followed to the real icon
// and saved under the requested name.
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [, , iconsDir, outDir, ...names] = process.argv;
if (!iconsDir || !outDir || names.length === 0) {
  console.error('usage: node lucide2svg.mjs <lucide-react icons dir> <out dir> <icon names…>');
  process.exit(1);
}
for (const name of names) {
  let file = resolve(iconsDir, `${name}.mjs`);
  const alias = readFileSync(file, 'utf8').match(/export \{ default \} from '\.\/([a-z0-9-]+)\.mjs'/);
  if (alias) file = resolve(iconsDir, `${alias[1]}.mjs`);
  const { __iconNode } = await import(pathToFileURL(file).href);
  const body = __iconNode
    .map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).filter(([k]) => k !== 'key').map(([k, v]) => `${k}="${v}"`).join(' ')}/>`)
    .join('');
  writeFileSync(
    resolve(outDir, `${name}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>\n`,
  );
}
