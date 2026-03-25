# Solfunmeme Dioxus Plugins

12 registered plugins across 4 categories. Each plugin declares its capabilities
and is verified against zkperf witnesses.

## Registry

| # | Plugin | Category | Icon | Description | Security Tier |
|---|--------|----------|------|-------------|---------------|
| 1 | dao_governance | DAO | 🏛️ | Vote on daily ZK rollup bills | HIGH |
| 2 | pastebin | Data | 📋 | Submit TX data, earn bounties | HIGH |
| 3 | p2p_sharing | Data | 🌐 | P2P data sharing + stego | HIGH |
| 4 | embedding | Data | 🔢 | Token tier viewer (erdfa-publish) | MEDIUM |
| 5 | wikidata | Data | 🌐 | Wikidata concept browser | MEDIUM |
| 6 | doc_cleaner | Data | 📄 | Founding document browser | MEDIUM |
| 7 | markdown | Data | 📝 | Markdown renderer | MEDIUM |
| 8 | rust_parser | Analysis | 🔬 | Rust source code parser | MEDIUM |
| 9 | coverage | Analysis | 📊 | Code coverage viewer | MEDIUM |
| 10 | bert_test | Analysis | 🧠 | BERT embedding test | MEDIUM |
| 11 | monster_meme | Meta | 👹 | Monster Group meme generator | LOW |
| 12 | mcp | Meta | 🤖 | Model Context Protocol playground | LOW |

## Desktop-Only (WASM-gated)

| Plugin | Lines | Why gated |
|--------|-------|-----------|
| solfunmeme | 825 | dioxus-motion animations |
| solfunnice | 1,413 | dioxus-motion animations |
| orbits | 1,157 | dioxus-motion animations |
| test_emojis | 1,307 | dioxus-motion animations |

## Files

- [SECURITY_POLICY.md](SECURITY_POLICY.md) — capability matrix + threat model
- Source: `src/plugin.rs` — registry + PluginBrowser component
- Route: `/plugins` — live plugin browser
- Live: https://solana.solfunmeme.com/dioxus/plugins

## Architecture

```
register_plugin! macro
    ↓
inventory::collect!(PluginRegistration)
    ↓
PluginBrowser component (/plugins route)
    ↓
zkperf witness verification (planned)
    ↓
DAO senate audit (planned)
```
