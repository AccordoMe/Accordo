# Third-party material

Shipped inside the binary:

| What | Where | Licence |
|---|---|---|
| Inter 4.1 font (Rasmus Andersson) | `assets/fonts/` | SIL Open Font License 1.1 — `assets/fonts/LICENSE.txt` |
| Lucide icons | `assets/icons/` | ISC (parts MIT, from Feather) — `assets/icons/LICENSE` |
| Emoji names and keywords from emoji-mart's data (Missive), made with `scripts/emoji_data.mjs` | `assets/emoji.json` | MIT — `assets/emoji.LICENSE` |

Rust dependencies are fetched by Cargo; their licences are all permissive and compatible
with the GPL v3 (MIT, Apache-2.0, ISC, BSD-3-Clause, Zlib, BSL-1.0, Unicode-3.0,
CDLA-Permissive-2.0). To list them for a release:

```sh
cargo metadata --format-version 1 | jq -r '.packages[] | "\(.name) \(.version): \(.license)"' | sort
```

(or generate a full notice file with [`cargo about`](https://github.com/EmbarkStudios/cargo-about)).
