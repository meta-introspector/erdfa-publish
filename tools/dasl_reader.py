"""DASL/CBOR shard reader for neural-moonshine pipeline.

Reads 0xDA51-tagged CBOR shards and feeds binary content
through the Golay G24 SNN encoder/decoder.

DASL address format (64 bits):
  [prefix:16=0xDA51][type:4][data:44]

CBOR shard format:
  Tag(55889, {id, cid, component, tags})
"""

import struct
import json
import sys
import os
from pathlib import Path

try:
    import cbor2
except ImportError:
    cbor2 = None

DASL_TAG = 55889  # 0xDA51

TYPE_NAMES = {
    0: "MonsterWalk",
    1: "ASTNode",
    2: "Protocol",
    3: "NestedCID",
    4: "HarmonicPath",
    5: "ShardID",
    6: "Eigenspace",
    7: "Hauptmodul",
}

SSP = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 41, 47, 59, 71]

EIGENSPACES = {0: "Earth", 1: "Spoke", 2: "Hub", 3: "Clock"}

BOTT = ["R", "C", "H", "H⊕H", "H(2)", "C(4)", "R(8)", "R(8)⊕R(8)"]


def parse_dasl_address(addr: int) -> dict:
    """Parse a 64-bit DASL address into its fields."""
    prefix = (addr >> 48) & 0xFFFF
    if prefix != 0xDA51:
        return {"error": f"bad prefix 0x{prefix:04X}", "raw": addr}

    type_field = (addr >> 44) & 0xF
    data = addr & 0xFFFFFFFFFFF  # 44 bits

    result = {"prefix": "0xDA51", "type": type_field,
              "type_name": TYPE_NAMES.get(type_field, "Unknown"),
              "address": f"0x{addr:016X}"}

    if type_field == 0:  # Monster Walk
        result["group"] = (data >> 40) & 0xF
        result["position"] = (data >> 32) & 0xFF
        result["sequence"] = (data >> 16) & 0xFFFF
        result["factors"] = (data >> 12) & 0xF
    elif type_field == 1:  # AST Node
        result["selector"] = (data >> 41) & 0x7
        result["bott"] = (data >> 38) & 0x7
        result["bott_name"] = BOTT[(data >> 38) & 0x7]
        result["tenfold"] = (data >> 27) & 0x7FF
        result["hecke"] = (data >> 20) & 0x7F
        result["hecke_prime"] = SSP[min((data >> 20) & 0x7F, 14)]
        result["hash"] = data & 0xFFFFF
    elif type_field == 3:  # Nested CID
        result["shard"] = (data >> 36) & 0xFF
        result["hecke"] = (data >> 28) & 0xFF
        result["bott"] = (data >> 20) & 0xFF
        result["hash"] = data & 0xFFFFF
    elif type_field == 5:  # Shard ID
        result["prime_idx"] = (data >> 40) & 0xF
        result["prime"] = SSP[min((data >> 40) & 0xF, 14)]
        result["replica"] = (data >> 36) & 0xF
        result["zone"] = (data >> 28) & 0xFF
        result["node"] = data & 0xFFFFFFF
    elif type_field == 6:  # Eigenspace
        result["eigenspace"] = (data >> 42) & 0x3
        result["eigenspace_name"] = EIGENSPACES.get((data >> 42) & 0x3)
        result["prime_idx"] = (data >> 38) & 0xF
        result["prime"] = SSP[min((data >> 38) & 0xF, 14)]
        result["mckay"] = (data >> 32) & 0x3F
        result["hub_proj"] = (data >> 28) & 0xF
        result["hash"] = data & 0xFFFFFFF
    elif type_field == 7:  # Hauptmodul
        result["prime_idx"] = (data >> 40) & 0xF
        result["prime"] = SSP[min((data >> 40) & 0xF, 14)]
        result["genus"] = (data >> 36) & 0xF
        result["coeff_idx"] = (data >> 28) & 0xFF
        result["coeff_val"] = data & 0xFFFFFFF

    return result


def read_cbor_shard(path: str) -> dict:
    """Read a DA51-tagged CBOR shard file."""
    if cbor2 is None:
        raise ImportError("pip install cbor2")

    with open(path, "rb") as f:
        raw = f.read()

    obj = cbor2.loads(raw)

    # cbor2 returns Tag objects for tagged values
    if hasattr(obj, "tag") and obj.tag == DASL_TAG:
        return obj.value
    # Already decoded map
    if isinstance(obj, dict):
        return obj
    return {"raw": obj}


def shard_to_binary(shard: dict) -> bytes:
    """Extract binary content from a shard for SNN encoding."""
    if "component" in shard:
        comp = shard["component"]
        # Extract text content from any component type
        for key in ("text", "source", "label"):
            if key in comp:
                return comp[key].encode("utf-8")
        # Fallback: serialize the whole component
        return json.dumps(comp, sort_keys=True).encode("utf-8")
    return json.dumps(shard, sort_keys=True).encode("utf-8")


def read_json_block(path: str) -> dict:
    """Read a JSON block file (from erdfa/blocks/)."""
    with open(path) as f:
        return json.load(f)


def json_block_to_binary(block: dict) -> bytes:
    """Extract binary content from a JSON block."""
    if "content" in block:
        return block["content"].encode("utf-8")
    return json.dumps(block, sort_keys=True).encode("utf-8")


def scan_shards(directory: str):
    """Scan a directory for CBOR shards and JSON blocks."""
    p = Path(directory)
    shards = []
    for f in sorted(p.rglob("*.cbor")):
        try:
            s = read_cbor_shard(str(f))
            s["_source"] = str(f)
            shards.append(("cbor", s))
        except Exception as e:
            print(f"WARN: {f}: {e}", file=sys.stderr)
    for f in sorted(p.rglob("*.json")):
        try:
            b = read_json_block(str(f))
            b["_source"] = str(f)
            shards.append(("json", b))
        except Exception as e:
            print(f"WARN: {f}: {e}", file=sys.stderr)
    return shards


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="DASL/CBOR shard reader")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_addr = sub.add_parser("addr", help="Parse a DASL address")
    p_addr.add_argument("address", help="Hex address (0xDA51...)")

    p_read = sub.add_parser("read", help="Read a CBOR shard")
    p_read.add_argument("path", help="Path to .cbor file")

    p_scan = sub.add_parser("scan", help="Scan directory for shards")
    p_scan.add_argument("directory")

    p_export = sub.add_parser("export", help="Export shards as binary for SNN")
    p_export.add_argument("directory", help="Source directory")
    p_export.add_argument("output", help="Output binary file")

    args = parser.parse_args()

    if args.cmd == "addr":
        addr = int(args.address, 16) if args.address.startswith("0x") else int(args.address)
        print(json.dumps(parse_dasl_address(addr), indent=2))

    elif args.cmd == "read":
        shard = read_cbor_shard(args.path)
        print(json.dumps(shard, indent=2, default=str))

    elif args.cmd == "scan":
        shards = scan_shards(args.directory)
        for kind, s in shards:
            src = s.pop("_source", "?")
            cid = s.get("cid", "?")
            sid = s.get("id", s.get("title", "?"))
            print(f"[{kind}] {src}  id={sid}  cid={cid}")
        print(f"\nTotal: {len(shards)} shards")

    elif args.cmd == "export":
        shards = scan_shards(args.directory)
        with open(args.output, "wb") as out:
            for kind, s in shards:
                if kind == "cbor":
                    out.write(shard_to_binary(s))
                else:
                    out.write(json_block_to_binary(s))
        sz = os.path.getsize(args.output)
        print(f"Exported {len(shards)} shards → {args.output} ({sz} bytes)")
