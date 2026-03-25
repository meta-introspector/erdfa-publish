# NotebookLM Source Preparation Makefile
# Generates text sources for Google NotebookLM ingestion
# Output: /var/www/solana.solfunmeme.com/notebooklm/
# URL: https://solana.solfunmeme.com/notebooklm/

NBLM_DIR := /var/www/solana.solfunmeme.com/notebooklm
DATE := $(shell date +%Y%m%d)
PROOFS := $(HOME)/.solfunmeme/proofs
LEAN4 := /mnt/data1/meta-introspector/submodules/solfunmeme-introspector/SolfunmemeLean
DIOXUS := /mnt/data1/meta-introspector/submodules/solfunmeme-dioxus
DOCS := $(HOME)/DOCS/services/solfunmeme-dioxus

.PHONY: all lean4 proofs plugins security codebase report federal clean list

all: lean4 proofs plugins security codebase report federal
	@echo "✓ All NotebookLM sources generated in $(NBLM_DIR)"
	@echo "  $(shell ls $(NBLM_DIR)/solfunmeme-*.txt 2>/dev/null | wc -l) solfunmeme files"

# ── Lean4 proofs as single text file ──────────────────────────────
lean4: $(NBLM_DIR)/solfunmeme-lean4-proofs-$(DATE).txt

$(NBLM_DIR)/solfunmeme-lean4-proofs-$(DATE).txt:
	@echo "Generating Lean4 proofs source..."
	@{ echo "# SOLFUNMEME Lean4 Verified Proofs"; \
	   echo "# Generated: $(DATE)"; echo ""; \
	   for f in FederalModel Governance FederalGov Bills VotingProtocol; do \
	     echo "## $$f.lean"; echo '```lean4'; \
	     cat $(LEAN4)/$$f.lean; \
	     echo '```'; echo ""; \
	   done; } > $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines)"

# ── Proof artifacts as readable text ──────────────────────────────
proofs: $(NBLM_DIR)/solfunmeme-proof-artifacts-$(DATE).txt

$(NBLM_DIR)/solfunmeme-proof-artifacts-$(DATE).txt:
	@echo "Generating proof artifacts source..."
	@{ echo "# SOLFUNMEME Proof Artifacts"; \
	   echo "# Generated: $(DATE)"; echo ""; \
	   for f in completeness_proof credentials tally; do \
	     echo "## $$f.json"; \
	     python3 -m json.tool $(PROOFS)/$$f.json 2>/dev/null || cat $(PROOFS)/$$f.json; \
	     echo ""; \
	   done; \
	   echo "## holder_identities.json (summary)"; \
	   python3 -c "import json; d=json.load(open('$(PROOFS)/holder_identities.json')); print(json.dumps(d['classification'],indent=2)); print(f'Total: {d[\"total_holders\"]} holders')" 2>/dev/null; \
	   } > $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines)"

# ── Plugin registry + docs ────────────────────────────────────────
plugins: $(NBLM_DIR)/solfunmeme-plugins-$(DATE).txt

$(NBLM_DIR)/solfunmeme-plugins-$(DATE).txt:
	@echo "Generating plugin docs source..."
	@{ echo "# SOLFUNMEME Dioxus Plugin Registry"; \
	   echo "# Generated: $(DATE)"; echo ""; \
	   cat $(DOCS)/plugins/README.md; echo ""; \
	   for f in $(DOCS)/plugins/*.md; do \
	     [ "$$(basename $$f)" = "README.md" ] && continue; \
	     [ "$$(basename $$f)" = "SECURITY_POLICY.md" ] && continue; \
	     echo "---"; cat $$f; echo ""; \
	   done; } > $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines)"

# ── Security policy ───────────────────────────────────────────────
security: $(NBLM_DIR)/solfunmeme-security-policy-$(DATE).txt

$(NBLM_DIR)/solfunmeme-security-policy-$(DATE).txt:
	@cp $(DOCS)/plugins/SECURITY_POLICY.md $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines)"

# ── Codebase map ──────────────────────────────────────────────────
codebase: $(NBLM_DIR)/solfunmeme-codebase-map-$(DATE).txt

$(NBLM_DIR)/solfunmeme-codebase-map-$(DATE).txt:
	@cp $(DIOXUS)/CODEBASE_MAP.md $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines)"

# ── Federal model report ──────────────────────────────────────────
report: $(NBLM_DIR)/solfunmeme-federal-report-$(DATE).txt

$(NBLM_DIR)/solfunmeme-federal-report-$(DATE).txt:
	@echo "Generating federal model report..."
	@{ echo "# SOLFUNMEME Federal Model Report"; \
	   echo "# Generated: $(DATE)"; echo ""; \
	   echo "## Token"; \
	   echo "Mint: BwUTq7fS6sfUmHDwAiCQZ3asSiPEapW5zDrsbwtapump"; \
	   echo "Supply: 999,791,488.86 tokens (mint renounced)"; echo ""; \
	   echo "## Data Coverage"; \
	   python3 -c "import json; d=json.load(open('$(PROOFS)/completeness_proof.json')); \
	     print(f'TX analyzed: {d[\"coverage\"][\"tx_files_analyzed\"]}'); \
	     print(f'Coverage: {d[\"coverage\"][\"coverage_pct\"]:.1f}%'); \
	     print(f'Actors: {d[\"actors\"][\"total_unique_actors\"]}'); \
	     print(f'Holders: {d[\"actors\"][\"holders_with_positive_balance\"]}')"; echo ""; \
	   echo "## Holder Classification"; \
	   python3 -c "import json; d=json.load(open('$(PROOFS)/holder_identities.json')); \
	     [print(f'{k}: {v}') for k,v in d['classification'].items()]"; echo ""; \
	   echo "## Governance (Lean4 Verified)"; \
	   echo "Senate: top 100 holders, 51/100 majority"; \
	   echo "House: next 500 holders, 251/500 majority"; \
	   echo "Lobby: next 1000 holders, advisory"; \
	   echo "Veto override: 67 senate + 334 house"; echo ""; \
	   echo "## Credentials"; \
	   python3 -c "import json; d=json.load(open('$(PROOFS)/credentials.json')); \
	     c=d['credentials']; print(f'Total: {c[\"total\"]} (senate={c[\"senate\"]}, house={c[\"house\"]}, lobby={c[\"lobby\"]})')"; \
	   } > $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines)"

# ── Full combined source (all-in-one) ─────────────────────────────
federal: $(NBLM_DIR)/solfunmeme-complete-$(DATE).txt

$(NBLM_DIR)/solfunmeme-complete-$(DATE).txt: lean4 proofs plugins security codebase report
	@cat $(NBLM_DIR)/solfunmeme-federal-report-$(DATE).txt \
	     $(NBLM_DIR)/solfunmeme-lean4-proofs-$(DATE).txt \
	     $(NBLM_DIR)/solfunmeme-security-policy-$(DATE).txt \
	     $(NBLM_DIR)/solfunmeme-plugins-$(DATE).txt > $@
	@echo "  ✓ $@ ($(shell wc -l < $@) lines) — combined source"

# ── Utilities ─────────────────────────────────────────────────────
list:
	@echo "NotebookLM sources in $(NBLM_DIR):"
	@ls -lhS $(NBLM_DIR)/solfunmeme-*.txt 2>/dev/null || echo "  (none yet)"

clean:
	rm -f $(NBLM_DIR)/solfunmeme-*-$(DATE).txt
	@echo "Cleaned today's files"
