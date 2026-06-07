#!/usr/bin/env python3
"""One-shot converter: runtime_manifest.toml -> runtime_manifest.bsol."""

from __future__ import annotations

import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    import tomli as tomllib  # type: ignore


def fmt_ident(value: str) -> str:
    if value in {"true", "false", "never", "unit", "ptr", "language", "host"}:
        return value
    if value.replace("_", "").isalnum():
        return value
    return f'"{value}"'


def fmt_list(values: list[str]) -> str:
    if not values:
        return "[]"
    return "[" + ", ".join(fmt_ident(v) for v in values) + "]"


def emit_kernel(entry: dict) -> list[str]:
    lines = ["kernel {"]
    lines.append(f'  symbol = {fmt_ident(entry["symbol"])}')
    lines.append(f'  name = {fmt_ident(entry["name"])}')
    lines.append(f'  params = {fmt_list(entry.get("params", []))}')
    lines.append(f'  returns = {fmt_ident(entry["returns"])}')
    lines.append(f'  injected = {str(entry.get("injected", False)).lower()}')
    if entry.get("beskid_path"):
        lines.append(f'  beskid_path = {fmt_list(entry["beskid_path"])}')
    lines.append("}")
    return lines


def emit_dispatch(kind: str, entry: dict) -> list[str]:
    block = f"dispatch_{kind}"
    lines = [f"{block} {{"]
    lines.append(f'  tag = {entry["tag"]}')
    lines.append(f'  dispatch_key = {fmt_ident(entry["dispatch_key"])}')
    lines.append(f'  name = {fmt_ident(entry["name"])}')
    lines.append(f'  params = {fmt_list(entry.get("params", []))}')
    lines.append(f'  returns = {fmt_ident(entry["returns"])}')
    lines.append(f'  injected = {str(entry.get("injected", True)).lower()}')
    if entry.get("beskid_path"):
        lines.append(f'  beskid_path = {fmt_list(entry["beskid_path"])}')
    if entry.get("owner", "language") != "language":
        lines.append(f'  owner = {fmt_ident(entry["owner"])}')
    lines.append("}")
    return lines


def emit_intrinsic(entry: dict) -> list[str]:
    lines = ["intrinsic {"]
    lines.append(f'  symbol = {fmt_ident(entry["symbol"])}')
    lines.append(f'  path = {fmt_list(entry["path"])}')
    lines.append(f'  params = {fmt_list(entry.get("params", []))}')
    lines.append(f'  returns = {fmt_ident(entry["returns"])}')
    lines.append(f'  injected = {str(entry.get("injected", False)).lower()}')
    lines.append("}")
    return lines


def convert(data: dict) -> str:
    out: list[str] = []
    manifest = data["manifest"]
    out.append("manifest {")
    out.append(f'  abi_version = {manifest["abi_version"]}')
    out.append("}")
    out.append("")

    profiles = data.get("profiles", {})
    for name in ("minimal", "std"):
        if name not in profiles:
            continue
        owners = profiles[name].get("owners", [])
        out.append(f'profile "{name}" {{')
        out.append(f"  owners = {fmt_list(owners)}")
        out.append("}")
        out.append("")

    for entry in data.get("kernel", []):
        out.extend(emit_kernel(entry))
        out.append("")

    dispatch = data.get("dispatch", {})
    for kind in ("usize", "ptr", "unit", "i64"):
        for entry in dispatch.get(kind, []):
            out.extend(emit_dispatch(kind, entry))
            out.append("")

    for entry in data.get("intrinsic", []):
        out.extend(emit_intrinsic(entry))
        out.append("")

    return "\n".join(out).rstrip() + "\n"


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    src = root / "runtime_manifest.toml"
    dst = root / "runtime_manifest.bsol"
    if not src.exists():
        print(f"missing {src}", file=sys.stderr)
        return 1
    data = tomllib.loads(src.read_text(encoding="utf-8"))
    dst.write_text(convert(data), encoding="utf-8")
    print(f"wrote {dst}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
