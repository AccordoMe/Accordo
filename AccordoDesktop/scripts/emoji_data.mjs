// Writes assets/emoji.json for the emoji picker from emoji-mart's data (the web app's picker).
//
//   node scripts/emoji_data.mjs ../node_modules/@emoji-mart/data/sets/15/native.json assets/emoji.json
//
// Shape: {"categories": [{"id": "people", "emojis": [[native, name, keywords, [skin variants…]], …]}, …]}
// (skin variants only for emoji that have them: light → dark).
import { readFileSync, writeFileSync } from 'node:fs';

const [, , input, output] = process.argv;
if (!input || !output) {
  console.error('usage: node emoji_data.mjs <emoji-mart native.json> <out.json>');
  process.exit(1);
}
const data = JSON.parse(readFileSync(input, 'utf8'));
const categories = data.categories.map((c) => ({
  id: c.id,
  emojis: c.emojis
    .map((id) => data.emojis[id])
    .filter(Boolean)
    .map((e) => {
      const row = [e.skins[0].native, e.name, [...new Set([e.id, ...(e.keywords || [])])].join(' ')];
      if (e.skins.length > 1) row.push(e.skins.slice(1).map((s) => s.native));
      return row;
    }),
}));
writeFileSync(output, JSON.stringify({ categories }));
console.log(`${categories.reduce((n, c) => n + c.emojis.length, 0)} emoji in ${categories.length} categories`);
