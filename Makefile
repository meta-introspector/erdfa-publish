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
