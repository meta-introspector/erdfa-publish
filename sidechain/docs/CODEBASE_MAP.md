# solfunmeme-dioxus Codebase Map

**Total: 33,829 lines, 100+ Rust files**
**Build: `nix develop --command dx build --release --platform web`**
**Live: https://solana.solfunmeme.com/dioxus/**

## Architecture

```
src/
├── main.rs              — entry point, launches PlaygroundApp
├── lib.rs               — lib crate (core module)
├── app.rs               — Route enum, MainApp (wallet event loop)
├── header.rs        421 — nav bar, wallet connect button
├── fetch_parser.rs      — Solana RPC fetch (getSignatures, getBalance, etc.)
├── fetch_util.rs        — HTTP fetch helpers
├── password_manager.rs 959 — encrypted password vault (AES-GCM)
├── svg_assets.rs    774 — inline SVG icons
├── embedself.rs         — self-embedding (prints own source)
├── state/               — app state management
├── stubs/               — dioxus-motion stubs (animation disabled for 0.7)
│
├── model/           4,424 lines — data types + business logic
│   ├── erdfa.rs         — re-exports from erdfa-publish (shared types)
│   ├── rpcreponse.rs    — RpcResponse<T>
│   ├── signaturesresponse.rs — SignaturesResponse
│   ├── accountstate.rs  — wallet account state
│   ├── adaptercluster.rs 200 — RPC cluster config
│   ├── use_connections.rs 171 — connection management hooks
│   ├── metameme.rs      783 — MetaMeme ontology (language mappings, AST)
│   ├── prime_ontology.rs 344 — prime number → semantic concept mapping
│   ├── ontology_mcp_bridge.rs 698 — MCP protocol bridge
│   ├── simple_expr.rs   781 — expression parser/evaluator
│   ├── clifford.rs      356 — Clifford algebra operations
│   ├── crypto.rs        414 — encryption (AES-GCM, ChaCha20, X25519)
│   ├── wasm_bert.rs     486 — BERT embeddings in WASM
│   ├── theme_node.rs    150 — UI theme tree
│   ├── lean/            — Lean4 integration (parser, style, emoji tests)
│   └── storage.rs       — global signals (WALLET_ADAPTER, ACCOUNT_STATE)
│
├── views/           4,547 lines — UI components (routes)
│   ├── dashboard.rs      46 — home page
│   ├── accounts.rs      423 — wallet account display
│   ├── clusters.rs      308 — RPC cluster management
│   ├── extras.rs         24 — extras menu
│   ├── source_browser.rs 242 — source code viewer
│   ├── dao_governance.rs  67 — DAO voting (NEW)
│   ├── pastebin_view.rs   96 — tx submission + bounty (NEW)
│   ├── p2p_sharing.rs    78 — P2P data network (NEW)
│   ├── lean.rs          531 — Lean4 proof viewer
│   ├── memes.rs         699 — meme browser/editor
│   ├── meme_management.rs 481 — meme CRUD
│   ├── send_sol.rs      123 — send SOL transaction
│   ├── receive_sol.rs    81 — receive SOL (QR code)
│   ├── airdrop.rs       148 — devnet airdrop
│   ├── crypto_frontend*.rs — encryption UI
│   ├── connection_*.rs  — cluster connection UI
│   └── notification.rs  130 — toast notifications
│
├── playground/      9,122 lines — experimental/demo components
│   ├── app.rs           186 — PlaygroundApp (menu router)
│   ├── solfunnice.rs  1,413 — main SolFunMeme app (WASM-gated)
│   ├── solfunmeme.rs   825 — token display (WASM-gated)
│   ├── orbits.rs      1,157 — orbital visualization (WASM-gated)
│   ├── polygon.rs       863 — polygon renderer
│   ├── test_emojis.rs 1,307 — emoji test suite (WASM-gated)
│   ├── test_components.rs 1,083 — component test harness
│   ├── test_app.rs    1,197 — test application
│   ├── coverage_app.rs  770 — code coverage viewer
│   ├── mcp.rs           834 — MCP protocol playground
│   ├── rust_parser.rs   609 — Rust source parser
│   ├── monster_meta_meme.rs 395 — Monster Group meme generator
│   ├── bert_test.rs     138 — BERT embedding test
│   ├── performance_charts.rs 195 — perf charts
│   ├── rust_bert_wasm.rs 373 — BERT WASM bindings
│   ├── embedding.rs      11 — embedding placeholder
│   ├── wikidata.rs       29 — Wikidata query
│   └── markdown_processor.rs 1 — empty
│
├── core/            1,821 lines — analysis engine
│   ├── code_analyzer.rs  317 — AST analysis
│   ├── declaration_splitter.rs 310 — split code into declarations
│   ├── duplicate_detector.rs 287 — find duplicate code
│   ├── meme_generator.rs 446 — generate memes from code
│   ├── vectorization.rs  123 — code → vector embedding
│   └── wallet_integration.rs 325 — wallet crypto (AES, key derivation)
│
├── extractor/       ~2,000 lines — markdown code extractor (WASM-gated)
│   ├── components/      — UI: dropzone, file display, code snippets
│   ├── model/           — file processing, download
│   ├── system/          — clipboard
│   └── types.rs         — ExtractedFile, CodeSnippet
│
└── bin/
    ├── doc_test_generator.rs — generates doc tests
    └── test_runner.rs   347 — test harness
```

## WASM-Gated Modules (desktop only)

These use dioxus-motion animations or proc_macro2 spans:
- `playground/solfunnice.rs` — main animation app
- `playground/solfunmeme.rs` — token animations
- `playground/orbits.rs` — orbital viz
- `playground/test_emojis.rs` — emoji tests
- `extractor/` — file upload (FileEngine API)

## Key Dependencies

| Dep | Purpose |
|-----|---------|
| dioxus 0.7.3 | UI framework (WASM) |
| solana-sdk 2.3.0 | Solana transactions |
| wallet-adapter | Phantom/Solflare wallet |
| erdfa-publish (wasm) | Shared types (tiers, stego) |
| ring + x25519-dalek | Crypto (encryption, key exchange) |
| syn + syn-serde | Rust source parsing |
| linfa | ML (PCA, preprocessing) |
| nalgebra + ndarray | Linear algebra |

## Cleanup Priorities

1. **playground/** (9,122 lines) — most is test/demo code, gate more for WASM
2. **model/simple_expr.rs** (781) — standalone expression parser, could be own crate
3. **model/metameme.rs** (783) — ontology, needs cleanup from PR #11 merge
4. **password_manager.rs** (959) — large, could be own crate
5. **svg_assets.rs** (774) — inline SVGs, move to assets/
6. **Empty/stub modules** — markdown_processor (1 line), doc_cleaner (6), embedding (11)
