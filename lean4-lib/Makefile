LEAN := $(HOME)/.elan/bin/lean
LEAN_PATH := $(CURDIR)
export LEAN_PATH

SRCS := DA51/CborVal.lean DA51/Encode.lean DA51/Decode.lean DA51/Reflect.lean \
        DA51/Conformal.lean DA51/Ternary.lean DA51/Monster.lean DA51/Hecke.lean \
        DA51/Bivector.lean DA51/Vertex.lean DA51/HVertex.lean DA51/Exec.lean \
        DA51/MultiAttractor.lean
OLEANS := $(SRCS:.lean=.olean)
TESTS := DA51/RoundTrip.lean DA51/RealRoundTrip.lean DA51/TestReflect.lean \
         DA51/SelfApply.lean DA51/Decode.lean DA51/Hecke.lean DA51/Bivector.lean \
         DA51/Vertex.lean DA51/HVertex.lean

.PHONY: all check test exec clean

all: $(OLEANS)

DA51/CborVal.olean: DA51/CborVal.lean
	$(LEAN) -o $@ $<
DA51/Encode.olean: DA51/Encode.lean DA51/CborVal.olean
	$(LEAN) -o $@ $<
DA51/Decode.olean: DA51/Decode.lean DA51/Encode.olean
	$(LEAN) -o $@ $<
DA51/Reflect.olean: DA51/Reflect.lean DA51/Encode.olean
	$(LEAN) -o $@ $<
DA51/Conformal.olean: DA51/Conformal.lean DA51/Reflect.olean
	$(LEAN) -o $@ $<
DA51/Ternary.olean: DA51/Ternary.lean DA51/Encode.olean
	$(LEAN) -o $@ $<
DA51/Monster.olean: DA51/Monster.lean DA51/Encode.olean
	$(LEAN) -o $@ $<
DA51/Hecke.olean: DA51/Hecke.lean DA51/Monster.olean
	$(LEAN) -o $@ $<
DA51/Bivector.olean: DA51/Bivector.lean DA51/Encode.olean
	$(LEAN) -o $@ $<
DA51/Vertex.olean: DA51/Vertex.lean DA51/Monster.olean
	$(LEAN) -o $@ $<
DA51/HVertex.olean: DA51/HVertex.lean DA51/Vertex.olean
	$(LEAN) -o $@ $<
DA51/Exec.olean: DA51/Exec.lean DA51/Decode.olean
	$(LEAN) -o $@ $<
DA51/MultiAttractor.olean: DA51/MultiAttractor.lean
	$(LEAN) -o $@ $<

check: $(OLEANS)
	@echo "All modules type-check ✓"

test: $(OLEANS)
	@for t in $(TESTS); do \
		echo "Testing $$t..."; \
		$(LEAN) $$t || exit 1; \
	done
	@echo "All tests pass ✓"

# Execute a DA51 CBOR shard: make exec SHARD=path/to/file.cbor
SHARD ?= ../shards/binding-ontology/clifford_rs.cbor
exec: DA51/Exec.olean
	$(LEAN) --run DA51/Exec.lean $(SHARD)

clean:
	rm -f DA51/*.olean DA51/*.ilean DA51/*.trace
