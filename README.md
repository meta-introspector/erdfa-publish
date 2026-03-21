# erdfa-publish

Semantic UI components as CBOR shards. Define structure in Rust, render anywhere.

Includes a **Conformal Field Tower (CFT)** module that decomposes any text into multi-scale layers — post, paragraph, line, token, emoji, bytes — with n-grams and typed arrows between layers. Every node and edge is a content-addressed DA51 CBOR shard.

## Concept

Instead of writing HTML/JS, you describe **what** your UI contains — headings, tables, trees, maps, code blocks — as typed Rust structs. These get serialized as CBOR shards with content-addressed IDs. Any renderer (browser, screen reader, CLI, embedded display) loads the shards and presents them according to its own a11y layer and CSS.

```
Rust program → Component structs → CBOR shards → loader → renderer
                                                          ├─ visual CSS
                                                          ├─ screen reader
                                                          ├─ CLI table
                                                          └─ braille display
```

## Install

```toml
[dependencies]
erdfa-publish = { git = "https://github.com/meta-introspector/erdfa-publish" }
```

## Quick start

```rust
use erdfa_publish::*;

// Create semantic components
let heading = Component::Heading { level: 1, text: "Results".into() };
let table = Component::Table {
    headers: vec!["Name".into(), "Value".into()],
    rows: vec![vec!["alpha".into(), "0.73".into()]],
};

// Wrap as shards (auto-generates CID from content hash)
let s1 = Shard::new("result-heading", heading);
let s2 = Shard::new("result-table", table).with_tags(vec!["data".into()]);

// Build manifest + tar archive
let mut set = ShardSet::new("my-results");
set.add(&s1);
set.add(&s2);
set.to_tar(&[s1, s2], std::fs::File::create("output.tar").unwrap()).unwrap();
```

## Conformal Field Tower (CFT)

Decompose any text into a tower of scale layers. Each layer is a shard, each edge is an arrow shard. N-grams (bigrams, trigrams) are computed at each level.

```
Scale 0: Post          "Hello world 🌍\n\nSecond paragraph."
  │                     bigrams: "Hello world" | "world 🌍" | ...
  ├─→ Scale 1: Paragraph₀   "Hello world 🌍"
  │     ├─→ Scale 2: Line₀       "Hello world 🌍"
  │     │     ├─→ Scale 3: Token₀    "Hello"
  │     │     │     └─→ Scale 5: Byte   "48 65 6c 6c 6f"
  │     │     ├─→ Scale 3: Token₁    "world"
  │     │     │     └─→ Scale 5: Byte   "77 6f 72 6c 64"
  │     │     └─→ Scale 3: Token₂    "🌍"
  │     │           ├─→ Scale 4: Emoji  [U+1F30D]
  │     │           └─→ Scale 5: Byte   "f0 9f 8c 8d"
  └─→ Scale 1: Paragraph₁   ...
```

### Usage

```rust
use erdfa_publish::cft;

let text = "Hello world 🌍\n\nThis is a test paragraph.\nWith two lines.";
let (shards, arrows) = cft::decompose("my-doc", text);

// shards: field nodes at every scale (Post, Paragraph, Line, Token, Emoji, Byte)
// arrows: typed edges between layers (parent→child with scale metadata)

// Every object is a DA51 CBOR shard
for shard in &shards {
    std::fs::write(
        format!("{}.cbor", shard.id),
        shard.to_cbor(),
    ).unwrap();
}
```

### Scale layers

| Scale | Depth | Splits on | N-grams | Component type |
|-------|-------|-----------|---------|---------------|
| Post | 0 | — | bigrams, trigrams of all tokens | KeyValue |
| Paragraph | 1 | `\n\n` | bigrams, trigrams | KeyValue |
| Line | 2 | `\n` | bigrams, trigrams | KeyValue |
| Token | 3 | whitespace | — | KeyValue |
| Emoji | 4 | unicode ranges | — | List (codepoints) |
| Byte | 5 | — | — | Code (hex) |

### Arrow shards

Every parent→child relationship is itself a shard:

```
DA51 tag → {
  "id": "my-doc_post→my-doc_p0",
  "component": {
    "type": "KeyValue",
    "pairs": [
      ["from", "my-doc_post"],
      ["to", "my-doc_p0"],
      ["scale_from", "0"],
      ["scale_to", "1"],
      ["morphism", "cft.post→cft.paragraph"]
    ]
  },
  "tags": ["cft", "arrow"]
}
```

### Scale as a functor

The decomposition is a functor from the category of texts to the category of shard diagrams. Each scale transformation (post→paragraph, paragraph→line, etc.) is a natural transformation. The arrows are morphisms. The n-grams are local invariants preserved across scales.

## Component types

| Type | Fields | Semantic meaning |
|------|--------|-----------------|
| `Heading` | `level`, `text` | Section header (1–6) |
| `Paragraph` | `text` | Block of prose |
| `Code` | `language`, `source` | Source code with syntax hint |
| `Table` | `headers`, `rows` | Tabular data |
| `Tree` | `label`, `children` | Recursive hierarchy |
| `List` | `ordered`, `items` | Ordered or unordered list |
| `Link` | `href`, `label` | Navigation reference |
| `Image` | `alt`, `cid` | Image by content address |
| `KeyValue` | `pairs` | Metadata / properties |
| `MapEntity` | `name`, `kind`, `x`, `y`, `meta` | Positioned entity on a map |
| `Group` | `role`, `children` | Container with semantic role |

## CBOR format

Every shard and manifest is wrapped in CBOR tag **55889** (`0xDA51`):

```
DA51 tag → {
  "id": "result-table",
  "cid": "bafk205260a6c670b02f...",
  "component": { "type": "Table", "headers": [...], "rows": [...] },
  "tags": ["data"]
}
```

## Tar archive layout

```
output.tar
├── result-heading.cbor    # DA51-tagged shard
├── result-table.cbor      # DA51-tagged shard
└── manifest.cbor          # DA51-tagged ShardSet
```

## Rendering

Shards are semantic, not visual. A loader fetches shards by CID, reads the `type` field, and delegates to the active a11y layer:

- **Visual**: CSS grid, syntax highlighting, interactive maps
- **Screen reader**: ARIA roles derived from component type
- **CLI**: ASCII tables, indented trees, plain text
- **Minimal**: progressive loading — show N/total progress

The `Group` component with a `role` field maps directly to ARIA landmarks (`navigation`, `main`, `complementary`, etc.).

## URLs

```rust
let shard = Shard::new("my-data", component);
shard.ipfs_url()                    // https://ipfs.io/ipfs/bafk...
shard.paste_url("http://host:8090") // http://host:8090/raw/my-data
```

## Tools

### `tools/dasl_reader.py` — Python DASL/CBOR reader

A standalone Python tool for reading DA51-tagged CBOR shards and parsing 64-bit DASL addresses.

Requires: `pip install cbor2` (or use the nix flake from [neural-moonshine](https://github.com/fargolo/neural-moonshine))

```bash
# Parse a DASL address
python3 tools/dasl_reader.py addr 0xDA510001F9080000

# Read a CBOR shard
python3 tools/dasl_reader.py read shards/note1.cbor

# Scan a directory for all shards
python3 tools/dasl_reader.py scan shards/

# Export shard content as binary
python3 tools/dasl_reader.py export shards/ output.bin
```

Supports all DASL address types: MonsterWalk, ASTNode, Protocol, NestedCID, HarmonicPath, ShardID, Eigenspace, Hauptmodul.

## License

MIT OR Apache-2.0
