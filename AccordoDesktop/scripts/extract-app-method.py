#!/usr/bin/env python3
"""Move inherent methods out of a Rust module file into the module's child files, verbatim.

    python3 scripts/extract-app-method.py src/app/mod.rs \
        wallet_update=src/app/wallet.rs vault_update=src/app/wallet.rs

Each argument is <method>=<destination file>. For every pair this script:

  * finds `    fn <method>(` (rustfmt indentation inside `impl`) and takes the block up to the
    matching closing brace, refusing to move anything that looks unbalanced;
  * rewrites the signature to `pub(super) fn` — a method defined in a child module is private
    to that module and would be invisible to the parent module that calls it;
  * deletes the block from the source and appends it, untouched, at the end of an
    `impl <Type> { … }` block in the destination file (created with `use super::*;` when the
    destination does not exist yet).

The body lines are copied byte for byte; only the signature keyword and the wrapping change.
Doc comments above the method do NOT travel with it — paste them into the destination and
delete them from the source, or clippy reports `empty_line_after_doc_comments`.

Afterwards add `mod <name>;` for every new file and run the gates in docs/app-modules.md.
"""

import re
import sys
from pathlib import Path


def impl_type_above(lines, fn_index):
    for i in range(fn_index, -1, -1):
        m = re.match(r'^impl(?:<[^>]*>)?\s+([A-Za-z_][\w:]*)\s*\{', lines[i])
        if m:
            return m.group(1)
    return None


def find_block(lines, name):
    start = next((i for i, l in enumerate(lines) if l.startswith(f'    fn {name}(')), None)
    if start is None:
        raise SystemExit(f'fn {name} not found at impl indentation (4 spaces)')
    depth = 0
    end = None
    for j in range(start, len(lines)):
        depth += lines[j].count('{') - lines[j].count('}')
        if depth == 0 and j > start:
            end = j
            break
    if end is None:
        raise SystemExit(f'fn {name}: unbalanced braces, refusing to move')
    return start, end


def main(argv):
    if len(argv) < 2 or '=' not in argv[1]:
        raise SystemExit(__doc__)
    src = Path(argv[0])
    lines = src.read_text().splitlines(keepends=True)
    moved = []
    for spec in argv[1:]:
        name, dest = spec.split('=', 1)
        start, end = find_block(lines, name)
        kind = impl_type_above(lines, start)
        if kind is None:
            raise SystemExit(f'fn {name}: no enclosing impl found')
        block = lines[start:end + 1]
        if not block[0].startswith('    fn '):
            raise SystemExit(f'fn {name}: signature line unexpected: {block[0]!r}')
        block[0] = block[0].replace('    fn ', '    pub(super) fn ', 1)
        del lines[start:end + 1]
        dest_path = Path(dest)
        text = ''.join(block).rstrip('\n')
        if dest_path.exists():
            existing = dest_path.read_text().rstrip('\n')
            if not existing.endswith('}'):
                raise SystemExit(f'{dest}: does not end with a closing brace, refusing to append')
            dest_path.write_text(existing[: existing.rfind('}')].rstrip('\n') + '\n\n' + text + '\n}\n')
        else:
            dest_path.parent.mkdir(parents=True, exist_ok=True)
            dest_path.write_text(
                '// SPDX-License-Identifier: GPL-3.0-or-later\n'
                f'//! Split out of {src.name}: see docs/app-modules.md.\n\n'
                'use super::*;\n\n'
                f'impl {kind} {{\n' + text + '\n}\n'
            )
        moved.append((name, kind, dest, len(block)))

    text = ''.join(lines)
    while '\n\n\n' in text:
        text = text.replace('\n\n\n', '\n\n')
    src.write_text(text)

    for name, kind, dest, n in moved:
        print(f'{name:24s} impl {kind:12s} -> {dest} ({n} lines)')
    print(f'{src}: {len(text.splitlines())} lines left')
    print('now: add `mod <name>;` for each new file, move the doc comments, run cargo check/test')


if __name__ == '__main__':
    main(sys.argv[1:])
