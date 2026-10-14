#!/usr/bin/env python3
"""Regenerate per-directory `tree` files from direct-file descriptions."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
RETIRED = {"apps/android", "apps/slint-ui"}
DESCRIPTION_FILE = ROOT / "scripts" / "tree_descriptions.json"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if any tree file is stale")
    args = parser.parse_args()

    descriptions = json.loads(DESCRIPTION_FILE.read_text(encoding="utf-8"))
    files = {
        path.relative_to(ROOT).as_posix()
        for path in ROOT.rglob("*")
        if path.is_file()
        and path.name != "tree"
        and not any(path.relative_to(ROOT).as_posix() == old or path.relative_to(ROOT).as_posix().startswith(old + "/") for old in RETIRED)
        and not {"target", ".git", "build", ".gradle", ".kotlin", "node_modules", "dist", "jniLibs", "schemas", "gen"}.intersection(path.relative_to(ROOT).parts)
    }
    missing = sorted(files - set(descriptions))
    stale = sorted(set(descriptions) - files)
    if missing or stale:
        if missing:
            print("Missing descriptions:\n" + "\n".join(missing), file=sys.stderr)
        if stale:
            print("Descriptions for missing files:\n" + "\n".join(stale), file=sys.stderr)
        return 1

    directories = {ROOT}
    directories.update(
        path for path in ROOT.rglob("*")
        if path.is_dir()
        and not any(path.relative_to(ROOT).as_posix() == old or path.relative_to(ROOT).as_posix().startswith(old + "/") for old in RETIRED)
        and not {"target", ".git", "build", ".gradle", ".kotlin", "node_modules", "dist", "jniLibs", "schemas", "gen"}.intersection(path.relative_to(ROOT).parts)
    )
    failures = []
    for directory in sorted(directories):
        relative_directory = directory.relative_to(ROOT)
        direct_files = sorted(
            (path for path in directory.iterdir() if path.is_file() and path.name != "tree"),
            key=lambda path: path.name,
        )
        lines = [
            "# 本目录直接文件说明",
            "",
            "仅列出本目录文件；子目录内容请查看对应目录中的 `tree`。",
            "",
        ]
        for path in direct_files:
            relative_path = path.relative_to(ROOT).as_posix()
            lines.append(f"- `{path.name}` — {descriptions[relative_path]}")
        lines.append("- `tree` — 本目录直接文件的说明索引。")
        content = "\n".join(lines) + "\n"
        tree_file = directory / "tree"
        if args.check:
            if not tree_file.exists() or tree_file.read_text(encoding="utf-8") != content:
                failures.append(relative_directory.as_posix() or ".")
        else:
            tree_file.write_text(content, encoding="utf-8")

    if failures:
        print("Out-of-date tree files:\n" + "\n".join(failures), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
