.PHONY: build develop check test clean run list show import parquet refresh index

build:
	nix build

develop:
	nix develop

check:
	nix develop -c cargo check

test:
	nix develop -c cargo test

clean:
	nix develop -c cargo clean
	rm -rf result

run:
	nix run

# CLI shortcuts — usage: make list DIR=./shards
list:
	nix run -- list $(DIR)

show:
	nix run -- show $(FILE)

import:
	nix run -- import --src $(SRC) --dir $(DIR) --max-depth $(or $(DEPTH),2)

parquet:
	nix run -- parquet --src $(SRC) --dir $(DIR) --max-depth $(or $(DEPTH),1)

refresh:
	nix run -- refresh --src $(SRC) --dir $(DIR) --max-depth $(or $(DEPTH),1)

index:
	nix run -- index --dir $(DIR) $(if $(OUT),--out $(OUT))

# cbor2agda — translate CBOR/DASL to Agda module
# usage: make cbor2agda FILE=foo.dasl [MOD=ModName] [OUT=Foo.agda]
cbor2agda:
	nix develop -c cargo run --bin cbor2agda -- $(FILE) $(MOD) $(OUT)

# Collect perf parquet as DA51 CBOR shards
# usage: make perf SRC=./parquet_dir DIR=./shards
perf:
	nix run -- perf --src $(SRC) --dir $(DIR)

# Export DA51 CBOR shards as Agda module
# usage: make agda DIR=./shards [OUT=PerfHistory.agda] [MODULE=PerfHistory]
agda:
	nix run -- agda --dir $(DIR) $(if $(OUT),--out $(OUT)) $(if $(MODULE),--module $(MODULE))

# ── solfunmeme-service ──────────────────────────────────────────

.PHONY: sf-build sf-serve sf-crawl sf-status sf-stop sf-logs sf-timer sf-hf-push

BIN := target/release/solfunmeme-service
BUDGET ?= 95000
RATE ?= 8

sf-build:
	nix develop -c cargo build --release --bin solfunmeme-service

sf-serve:
	systemctl --user restart solfunmeme-service
	systemctl --user status solfunmeme-service --no-pager

sf-stop:
	systemctl --user stop solfunmeme-service

sf-status:
	@systemctl --user status solfunmeme-service --no-pager 2>/dev/null || true
	@systemctl --user status solfunmeme-crawl.timer --no-pager 2>/dev/null || true
	@curl -s http://127.0.0.1:7780/status 2>/dev/null || echo "service not running"

sf-crawl:
	$(BIN) batch-crawl --budget $(BUDGET) --rate $(RATE)

sf-crawl-systemd:
	systemctl --user start solfunmeme-crawl.service
	journalctl --user -u solfunmeme-crawl -f --no-pager

sf-timer:
	systemctl --user enable --now solfunmeme-crawl.timer
	systemctl --user list-timers --no-pager | grep solfunmeme

sf-logs:
	journalctl --user -u solfunmeme-service -u solfunmeme-crawl -f --no-pager

sf-hf-push:
	cd ~/.solfunmeme/hf-dataset && git add -A && git commit -m "batch-crawl update $$(date -I)" && git push

# ── Prove + credentials ──────────────────────────────────────────

sf-prove:
	$(BIN) prove

sf-prove-systemd:
	systemctl --user start solfunmeme-prove.service
	journalctl --user -u solfunmeme-prove -f --no-pager

sf-prove-timer:
	systemctl --user enable --now solfunmeme-prove.timer

sf-collect:
	$(BIN) collect-votes

sf-pipeline:
	$(BIN) analyze && $(BIN) identify && $(BIN) prove && $(BIN) collect-votes

# ── CLI ───────────────────────────────────────────────────────────

sf-cli-build:
	nix develop -c cargo build --release --bin solfunmeme_cli

sf-cli-install:
	cp target/release/solfunmeme_cli ~/.local/bin/solfunmeme-cli
	@echo "✓ Installed to ~/.local/bin/solfunmeme-cli"
