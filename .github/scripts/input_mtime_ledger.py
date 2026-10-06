"""Opt-in tracked-input mtime reuse; never restore input bytes or permissions.

Only use in a trusted, immutable CI checkout: no concurrent source writers.
Cargo remains responsible for dependency and feature/env/flag fingerprints.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import stat
import subprocess

VERSION = 2
MAX_LEDGER_BYTES = 16 * 1024**2
MAX_INPUTS = 100_000
BASELINE_NAME = ".ci-workspace-inputs.json"
# Archived publication evidence is not a Rust/Slint build input.
# Never restore its timestamps: it retains ordinary checkout freshness. Still
# validate every index entry/path below before applying this exact exclusion.
PUBLICATION_PREFIXES = (
    "docs/rust/gui-coverage/checkpoints/",
    "docs/rust/gui-coverage/audit/",
)


def safe_path(root: Path, relative: str) -> Path:
    # Do not let Windows drives/UNC paths, separators or dot components alias a path.
    if not isinstance(relative, str) or not relative or "\\" in relative or ":" in relative or "\x00" in relative:
        raise ValueError("unsafe input path")
    parts = relative.split("/")
    if any(part in ("", ".", "..") for part in parts) or PurePosixPath(relative).is_absolute():
        raise ValueError("unsafe input path")
    path = root
    if root.is_symlink() or not root.is_dir():
        raise ValueError("unsafe checkout root")
    for index, part in enumerate(parts):
        path = path / part
        info = path.lstat()
        if stat.S_ISLNK(info.st_mode) or (index < len(parts) - 1 and not stat.S_ISDIR(info.st_mode)):
            raise ValueError("symlink/non-directory in input path")
    if not stat.S_ISREG(path.lstat().st_mode):
        raise ValueError("non-regular input")
    return path


def tracked_inputs(root: Path) -> dict[str, str]:
    raw = subprocess.check_output(["git", "ls-files", "--stage", "-z"], cwd=root)
    result: dict[str, str] = {}
    seen: set[str] = set()
    for record in raw.split(b"\x00"):
        if not record:
            continue
        header, raw_path = record.split(b"\t", 1)
        mode, _object, stage = header.decode("ascii").split()
        relative = raw_path.decode("utf-8")
        # Symlinks, submodules, conflicted indexes and duplicate paths fail closed.
        if mode not in ("100644", "100755") or stage != "0" or relative in seen:
            raise ValueError("unsupported Git input/index mode")
        seen.add(relative)
        if len(seen) > MAX_INPUTS:
            raise ValueError("unsupported input count")
        safe_path(root, relative)
        if relative.startswith(PUBLICATION_PREFIXES):
            continue
        result[relative] = mode
    if not result or len(result) > MAX_INPUTS:
        raise ValueError("unsupported input count")
    return result


def read_input(root: Path, relative: str, git_mode: str) -> dict:
    path = safe_path(root, relative)
    before = path.lstat()
    flags = os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0)
    descriptor = os.open(path, flags)
    try:
        opened = os.fstat(descriptor)
        if not stat.S_ISREG(opened.st_mode) or (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
            raise ValueError("input identity changed")
        digest = hashlib.sha256()
        while block := os.read(descriptor, 1024**2):
            digest.update(block)
        after = os.fstat(descriptor)
        if (opened.st_size, opened.st_mtime_ns, opened.st_ctime_ns, opened.st_mode) != (after.st_size, after.st_mtime_ns, after.st_ctime_ns, after.st_mode):
            raise ValueError("input changed during hashing")
        check = safe_path(root, relative).lstat()
        if (after.st_dev, after.st_ino) != (check.st_dev, check.st_ino):
            raise ValueError("input identity changed")
        identity = {"path": relative, "sha256": digest.hexdigest(), "git_mode": git_mode, "mode": stat.S_IMODE(after.st_mode)}
        identity["digest"] = hashlib.sha256(json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        return {**identity, "mtime_ns": after.st_mtime_ns}
    finally:
        os.close(descriptor)


def capture(root: Path) -> dict:
    return {"version": VERSION, "inputs": [read_input(root, path, mode) for path, mode in sorted(tracked_inputs(root).items())]}


def validate(ledger: dict) -> dict[str, dict]:
    if not isinstance(ledger, dict) or set(ledger) != {"version", "inputs"} or ledger["version"] != VERSION or not isinstance(ledger["inputs"], list):
        raise ValueError("invalid input ledger")
    if not 0 < len(ledger["inputs"]) <= MAX_INPUTS:
        raise ValueError("invalid input count")
    result = {}
    for entry in ledger["inputs"]:
        if not isinstance(entry, dict) or set(entry) != {"path", "sha256", "git_mode", "mode", "digest", "mtime_ns"}:
            raise ValueError("invalid input entry")
        relative = entry["path"]
        # Validate path syntax even before consulting the filesystem.
        if not isinstance(relative, str) or not relative or "\\" in relative or ":" in relative or "\x00" in relative or any(part in ("", ".", "..") for part in relative.split("/")):
            raise ValueError("unsafe ledger path")
        if relative in result or entry["git_mode"] not in ("100644", "100755"):
            raise ValueError("duplicate/unsupported ledger input")
        if type(entry["mode"]) is not int or not 0 <= entry["mode"] <= 0o7777 or type(entry["mtime_ns"]) is not int or not 0 < entry["mtime_ns"] <= 2**63 - 1:
            raise ValueError("invalid mode/timestamp")
        if not isinstance(entry["sha256"], str) or len(entry["sha256"]) != 64 or any(c not in "0123456789abcdef" for c in entry["sha256"]):
            raise ValueError("invalid content digest")
        identity = {key: entry[key] for key in ("path", "sha256", "git_mode", "mode")}
        expected = hashlib.sha256(json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        if entry["digest"] != expected:
            raise ValueError("input identity digest mismatch")
        result[relative] = entry
    return result


def load(path: Path) -> dict:
    if any(parent.is_symlink() for parent in path.parents) or path.is_symlink() or not path.is_file() or path.stat().st_size > MAX_LEDGER_BYTES:
        raise ValueError("unsafe/oversized input ledger")
    ledger = json.loads(path.read_text(encoding="utf-8"))
    validate(ledger)
    return ledger


def write(path: Path, ledger: dict) -> None:
    validate(ledger)
    encoded = json.dumps(ledger, sort_keys=True, indent=2).encode()
    if len(encoded) > MAX_LEDGER_BYTES or path.is_symlink():
        raise ValueError("unsafe/oversized ledger output")
    path.write_bytes(encoded)


def preflight(root: Path, cached: dict, newest_artifact_ns: int) -> tuple[dict, list[str]]:
    old = validate(cached)
    current = capture(root)
    now = validate(current)
    # Directory-watch behavior and permission changes are not inferred from mtimes.
    if old.keys() != now.keys():
        raise ValueError("input paths added/deleted; reject workspace snapshot")
    matching = []
    for path, entry in now.items():
        previous = old[path]
        if (entry["mode"], entry["git_mode"]) != (previous["mode"], previous["git_mode"]):
            raise ValueError("input mode changed; reject workspace snapshot")
        if entry["digest"] == previous["digest"]:
            matching.append(path)
        elif entry["mtime_ns"] <= newest_artifact_ns:
            raise ValueError("changed input is not newer than cached artifacts")
    return current, matching


def stamp_matching(root: Path, cached: dict, current: dict, matching: list[str]) -> int:
    old, now = validate(cached), validate(current)
    # Recheck every included input and validate all tracked paths before stamping.
    if not same_inputs(current, capture(root), include_mtime=True):
        raise ValueError("inputs changed after preflight")
    for relative in matching:
        previous = old[relative]
        entry = read_input(root, relative, now[relative]["git_mode"])
        if entry != now[relative] or entry["digest"] != previous["digest"]:
            raise ValueError("input no longer matches")
        path = safe_path(root, relative)
        # FD stamping prevents following a swapped symlink on supported platforms.
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0))
        try:
            opened = os.fstat(descriptor)
            check = safe_path(root, relative).lstat()
            if (opened.st_dev, opened.st_ino) != (check.st_dev, check.st_ino):
                raise ValueError("input identity changed before timestamp restore")
            if os.utime in os.supports_fd:
                os.utime(descriptor, ns=(opened.st_atime_ns, previous["mtime_ns"]))
            else:
                # Hosted Windows: trusted checkout must have no concurrent writers.
                os.utime(path, ns=(opened.st_atime_ns, previous["mtime_ns"]))
            if safe_path(root, relative).stat().st_mtime_ns != previous["mtime_ns"]:
                raise ValueError("filesystem cannot preserve recorded timestamp")
        finally:
            os.close(descriptor)
    return len(matching)


def same_inputs(left: dict, right: dict, *, include_mtime: bool = False) -> bool:
    first, second = validate(left), validate(right)
    if first.keys() != second.keys():
        return False
    return all(first[path] == second[path] if include_mtime else first[path]["digest"] == second[path]["digest"] for path in first)


def rollback_mtimes(root: Path, original: dict) -> list[str]:
    """Best-effort restoration of every modified original input, reporting failures."""
    errors = []
    for relative, previous in validate(original).items():
        try:
            entry = read_input(root, relative, previous["git_mode"])
            if entry["digest"] != previous["digest"]:
                raise ValueError("input contents/mode changed; cannot guarantee rollback")
            if entry["mtime_ns"] == previous["mtime_ns"]:
                continue
            path = safe_path(root, relative)
            descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_BINARY", 0) | getattr(os, "O_NOFOLLOW", 0))
            try:
                opened = os.fstat(descriptor)
                check = safe_path(root, relative).lstat()
                if (opened.st_dev, opened.st_ino) != (check.st_dev, check.st_ino):
                    raise ValueError("input identity changed before rollback")
                if os.utime in os.supports_fd:
                    os.utime(descriptor, ns=(opened.st_atime_ns, previous["mtime_ns"]))
                else:
                    os.utime(path, ns=(opened.st_atime_ns, previous["mtime_ns"]))
                if safe_path(root, relative).stat().st_mtime_ns != previous["mtime_ns"]:
                    raise ValueError("filesystem cannot restore original timestamp")
            finally:
                os.close(descriptor)
        except (OSError, ValueError, KeyError, TypeError) as error:
            errors.append(f"{relative}: {error}")
    try:
        if not same_inputs(original, capture(root), include_mtime=True):
            errors.append("full original input identity/timestamp check failed")
    except (OSError, ValueError, KeyError, TypeError) as error:
        errors.append(f"final input verification: {error}")
    return errors
