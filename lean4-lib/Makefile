LEAN := $(HOME)/.elan/bin/lean
LEAN_PATH := $(CURDIR)
export LEAN_PATH

SRCS := DA51/CborVal.lean DA51/Encode.lean DA51/Reflect.lean DA51/Conformal.lean DA51/Ternary.lean DA51/Monster.lean
OLEANS := $(SRCS:.lean=.olean)
TESTS := DA51/RoundTrip.lean DA51/RealRoundTrip.lean DA51/TestReflect.lean

.PHONY: all check test clean

all: $(OLEANS)

DA51/CborVal.olean: DA51/CborVal.lean
	$(LEAN) -o $@ $<

DA51/Encode.olean: DA51/Encode.lean DA51/CborVal.olean
	$(LEAN) -o $@ $<

DA51/Reflect.olean: DA51/Reflect.lean DA51/Encode.olean
	$(LEAN) -o $@ $<

DA51/Conformal.olean: DA51/Conformal.lean DA51/Reflect.olean
	$(LEAN) -o $@ $<

DA51/Ternary.olean: DA51/Ternary.lean DA51/Encode.olean
	$(LEAN) -o $@ $<

DA51/Monster.olean: DA51/Monster.lean DA51/Encode.olean
	$(LEAN) -o $@ $<

check: $(OLEANS)
	@echo "All modules type-check ✓"

test: $(OLEANS)
	@for t in $(TESTS); do \
		echo "Testing $$t..."; \
		$(LEAN) $$t || exit 1; \
	done
	@echo "All tests pass ✓"

clean:
	rm -f DA51/*.olean DA51/*.ilean DA51/*.trace
