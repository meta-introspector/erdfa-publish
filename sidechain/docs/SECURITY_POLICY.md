# Solfunmeme Dioxus Plugin Security Policy

## Threat Model

Each plugin runs as WASM in the browser. Risks:
1. **Data exfiltration** — plugin reads wallet keys or token balances
2. **UI spoofing** — plugin renders fake wallet connect dialogs
3. **Resource exhaustion** — plugin consumes CPU/memory, freezes browser
4. **Network abuse** — plugin makes unauthorized RPC calls

## Security Boundaries

### Tier 1: DAO (🏛️) — HIGH trust
Plugins that handle governance votes and token data.
- **Allowed**: Read wallet pubkey, read token balances, POST to /solfunmeme/paste
- **Denied**: Private key access, arbitrary network, localStorage write
- **Constraint**: max_render_ms = 500, max_fetch = 10/min

### Tier 2: Data (📊) — MEDIUM trust
Plugins that display/submit data.
- **Allowed**: Read-only RPC, POST to pastebin, read erdfa types
- **Denied**: Wallet signing, private key access, WebSocket
- **Constraint**: max_render_ms = 1000, max_fetch = 30/min

### Tier 3: Analysis (🔬) — MEDIUM trust
Plugins that parse/analyze code.
- **Allowed**: CPU computation, read source files, render charts
- **Denied**: Network access, wallet access, storage write
- **Constraint**: max_render_ms = 2000 (heavy computation allowed)

### Tier 4: Meta (🧬) — LOW trust
Experimental ontology/MCP modules.
- **Allowed**: Pure computation, render UI
- **Denied**: All network, all storage, all wallet
- **Constraint**: max_render_ms = 5000

### Tier 5: Test (🧪) — SANDBOXED
Test harnesses, no production access.
- **Allowed**: Render only
- **Denied**: Everything else
- **Constraint**: max_render_ms = 10000

## Verification

Each plugin must provide:
1. **Source hash** — SHA256 of source code at build time
2. **zkperf witness** — proves render time ≤ max_render_ms
3. **Capability declaration** — what APIs it claims to use
4. **Audit trail** — git commit + reviewer signature

## Enforcement

- `inventory` collects plugin metadata at compile time
- Runtime: measure actual render time, compare to max_render_ms
- Violation → plugin disabled + alert to DAO senate
- Quarterly audit: all plugins re-verified against source hash

## Plugin Capability Matrix

| Plugin | Network | Wallet | Storage | Compute | Max ms |
|--------|---------|--------|---------|---------|--------|
| dao_governance | POST paste | read pubkey | — | low | 500 |
| pastebin | POST paste | — | — | low | 500 |
| p2p_sharing | POST paste | read pubkey | — | low | 500 |
| embedding | — | — | — | low | 1000 |
| wikidata | GET wikidata | — | — | low | 1000 |
| doc_cleaner | — | — | — | low | 1000 |
| markdown | — | — | — | low | 1000 |
| rust_parser | — | — | — | high | 2000 |
| coverage | — | — | — | high | 2000 |
| bert_test | — | — | — | high | 2000 |
| monster_meme | — | — | — | med | 5000 |
| mcp | — | — | — | med | 5000 |

## Incident Response

1. Plugin exceeds constraint → auto-disable, log to journal
2. Senate votes to re-enable or permanently remove
3. Source audit required before re-enable
4. All incidents posted to pastebin for transparency
