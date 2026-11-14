import hashlib
import json
import sys
from pathlib import Path

TAG = 55889
MAX_BYTES = 4096
MAX_DEPTH = 8
MAX_ITEMS = 128
MAX_TEXT_BYTES = 1024
MAX_UINT = (1 << 64) - 1

class Reject(ValueError):
    pass

def head(major, n):
    if not isinstance(n, int) or n < 0 or n > MAX_UINT:
        raise Reject("NUMBER_OUT_OF_RANGE")
    prefix = major << 5
    if n < 24:
        return bytes([prefix | n])
    if n < 256:
        return bytes([prefix | 24, n])
    if n < 65536:
        return bytes([prefix | 25]) + n.to_bytes(2, "big")
    if n < 2**32:
        return bytes([prefix | 26]) + n.to_bytes(4, "big")
    return bytes([prefix | 27]) + n.to_bytes(8, "big")

def encode(value):
    if value is None:
        return b"\xf6"
    if value is True:
        return b"\xf5"
    if value is False:
        return b"\xf4"
    if isinstance(value, int):
        if value >= 0:
            return head(0, value)
        return head(1, -1 - value)
    if isinstance(value, bytes):
        return head(2, len(value)) + value
    if isinstance(value, str):
        raw = value.encode("utf-8")
        if len(raw) > MAX_TEXT_BYTES:
            raise Reject("TEXT_TOO_LARGE")
        return head(3, len(raw)) + raw
    if isinstance(value, list):
        if len(value) > MAX_ITEMS:
            raise Reject("ARRAY_TOO_LARGE")
        return head(4, len(value)) + b"".join(
            encode(v) for v in value
        )
    if isinstance(value, dict):
        if len(value) > MAX_ITEMS:
            raise Reject("MAP_TOO_LARGE")
        pairs = []
        for k, v in value.items():
            if not isinstance(k, str):
                raise Reject("NON_TEXT_KEY")
            key_bytes = encode(k)
            pairs.append((key_bytes, encode(v)))
        pairs.sort(key=lambda p: (len(p[0]), p[0]))
        return head(5, len(pairs)) + b"".join(
            k + v for k, v in pairs
        )
    raise Reject("UNSUPPORTED_VALUE")

def envelope(value):
    if not isinstance(value, dict):
        raise Reject("ROOT_NOT_MAP")
    return head(6, TAG) + encode(value)

class Decoder:
    def __init__(self, data):
        self.data = data
        self.pos = 0
        self.items = 0

    def take(self, count):
        if count < 0 or self.pos + count > len(self.data):
            raise Reject("TRUNCATED")
        result = self.data[self.pos:self.pos + count]
        self.pos += count
        return result

    def item(self, depth=0):
        if depth > MAX_DEPTH:
            raise Reject("DEPTH_EXCEEDED")
        self.items += 1
        if self.items > MAX_ITEMS:
            raise Reject("ITEM_LIMIT_EXCEEDED")

        start = self.pos
        first = self.take(1)[0]
        major, ai = first >> 5, first & 31

        if major == 7:
            if first == 0xf4:
                return False
            if first == 0xf5:
                return True
            if first == 0xf6:
                return None
            raise Reject("UNSUPPORTED_SIMPLE_OR_FLOAT")

        if ai < 24:
            n = ai
        elif ai in (24, 25, 26, 27):
            width = {24: 1, 25: 2, 26: 4, 27: 8}[ai]
            n = int.from_bytes(self.take(width), "big")
        else:
            raise Reject("INDEFINITE_OR_RESERVED")

        if self.data[start:self.pos] != head(major, n):
            raise Reject("NONMINIMAL_ENCODING")

        if major == 0:
            return n
        if major == 1:
            return -1 - n

        if major in (2, 3):
            if n > MAX_BYTES:
                raise Reject("LENGTH_LIMIT")
            raw = self.take(n)
            if major == 2:
                return raw
            if n > MAX_TEXT_BYTES:
                raise Reject("TEXT_LIMIT")
            try:
                return raw.decode("utf-8", errors="strict")
            except UnicodeDecodeError:
                raise Reject("INVALID_UTF8")

        if major == 4:
            if n > MAX_ITEMS:
                raise Reject("ARRAY_LIMIT")
            return [self.item(depth + 1) for _ in range(n)]

        if major == 5:
            if n > MAX_ITEMS:
                raise Reject("MAP_LIMIT")
            result = {}
            previous = None
            for _ in range(n):
                key_start = self.pos
                key = self.item(depth + 1)
                key_bytes = self.data[key_start:self.pos]
                if not isinstance(key, str):
                    raise Reject("NON_TEXT_MAP_KEY")
                rank = (len(key_bytes), key_bytes)
                if previous is not None and rank <= previous:
                    raise Reject("DUPLICATE_OR_UNORDERED_KEYS")
                previous = rank
                result[key] = self.item(depth + 1)
            return result

        raise Reject("NESTED_TAG_OR_UNSUPPORTED_MAJOR")

def validate(data):
    if not isinstance(data, bytes):
        raise Reject("INPUT_NOT_BYTES")
    if len(data) > MAX_BYTES:
        raise Reject("OBJECT_TOO_LARGE")
    expected_tag = head(6, TAG)
    if not data.startswith(expected_tag):
        raise Reject("INVALID_DA51_TAG")

    decoder = Decoder(data[len(expected_tag):])
    value = decoder.item()

    if decoder.pos != len(decoder.data):
        raise Reject("TRAILING_BYTES")

    if not isinstance(value, dict):
        raise Reject("ROOT_NOT_MAP")

    if envelope(value) != data:
        raise Reject("NONCANONICAL_OBJECT")

    return value

def sha(data):
    return hashlib.sha256(data).hexdigest()

def run_tests():
    cases = []

    def accepted(name, data):
        value = validate(data)
        assert envelope(value) == data
        cases.append({
            "name": name, "accepted": True,
            "sha256": sha(data)
        })
        print("ACCEPTED", name)

    def rejected(name, data):
        try:
            validate(data)
        except Reject as exc:
            cases.append({
                "name": name, "accepted": False,
                "reason": str(exc)
            })
            print("REJECTED", name, str(exc))
            return
        raise AssertionError("UNSAFE_ACCEPTANCE:" + name)

    accepted("EMPTY_DA51_MAP", envelope({}))
    accepted("CANONICAL_MAP", envelope({"a": 1, "b": 2}))
    accepted("UTF8_TEXT", envelope({"text": "λ∀∃"}))
    accepted("NESTED_VALUES", envelope({
        "data": [1, True, None, b"\x00\xff"]
    }))
    accepted("UINT64_MAX", envelope({"n": MAX_UINT}))

    rejected("TRAILING_BYTES", envelope({}) + b"\x00")
    rejected("WRONG_TAG", b"\xa0")
    rejected("NONMINIMAL_ZERO", head(6, TAG) +
             b"\xa1\x61n\x18\x00")
    rejected("DUPLICATE_KEYS", head(6, TAG) +
             b"\xa2\x61a\x01\x61a\x02")
    rejected("UNORDERED_KEYS", head(6, TAG) +
             b"\xa2\x61b\x02\x61a\x01")
    rejected("INVALID_UTF8", head(6, TAG) +
             b"\xa1\x61x\x61\xff")
    rejected("TRUNCATED", envelope({"n": 24})[:-1])
    rejected("FLOAT", head(6, TAG) +
             b"\xa1\x61x\xf9\x3c\x00")
    rejected("INDEFINITE_MAP", head(6, TAG) + b"\xbf\xff")
    rejected("NESTED_TAG", head(6, TAG) +
             b"\xa1\x61x\xd9\xda\x51\xa0")
    rejected("NON_TEXT_KEY", head(6, TAG) +
             b"\xa1\x01\x02")
    rejected("OVERSIZE", envelope({}) + b"x" * 4096)
    rejected("EXCESSIVE_DEPTH", envelope({
        "nested": [[[[[[[[[0]]]]]]]]]
    }))
    rejected("EXCESSIVE_ITEMS", envelope({
        "items": list(range(127))
    }))

    assert len(cases) == 19
    assert sum(c["accepted"] for c in cases) == 5
    assert sum(not c["accepted"] for c in cases) == 14
    return cases

if __name__ == "__main__":
    output = Path(sys.argv[1])
    cases = run_tests()
    report = {
        "stage": "PUB-06E-11C-01",
        "status": "EXPERIMENTAL_TEST_PASS",
        "profile": "strict bounded DA51 CBOR subset",
        "tag": TAG,
        "limits": {
            "bytes": MAX_BYTES,
            "depth": MAX_DEPTH,
            "items": MAX_ITEMS,
            "text_bytes": MAX_TEXT_BYTES
        },
        "tests": cases,
        "accepted": 5,
        "rejected": 14,
        "canonical_map_order": "encoded-key length then bytewise",
        "original_lean_decoder_modified": False,
        "lean_interoperability_verified": False,
        "semantic_witness_schema_verified": False,
        "implementation_formally_verified": False,
        "production_ready": False
    }
    output.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8"
    )
    print("STRICT_DA51_TESTS=19/19_PASS")
