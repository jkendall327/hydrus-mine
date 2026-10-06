"""Stage bounded workspace libraries; Cargo always checks restored artifacts.

Optional tracked-input ledger restores verified mtimes, never source bytes/modes.
Compiler flags, commands and Cargo fingerprint checks are unchanged.
The separate dependency cache must be restored before `restore` is invoked.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

import input_mtime_ledger as ledger

ROOT = Path.cwd()
SNAPSHOT = ROOT / ".ci-workspace-artifacts"
SCHEMA = 4
MAX_BYTES = 4 * 1024**3
PREFIXES = ("CARGO", "CC", "CFLAGS", "CXX", "CMAKE", "RUST", "SLINT", "PKG_CONFIG", "VCPKG", "SDKROOT", "MACOSX_DEPLOYMENT_TARGET")
LIB_SUFFIXES = (".rlib", ".rmeta", ".so", ".dylib", ".dll", ".lib", ".d")


def workspace_packages() -> tuple[set[str], set[str], list[Path]]:
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]
    excludes = {p.resolve() for pattern in workspace.get("exclude", []) for p in ROOT.glob(pattern)}
    members = sorted({p.resolve() for pattern in workspace["members"] for p in ROOT.glob(pattern)} - excludes)
    names: set[str] = set()
    crates: set[str] = set()
    manifests = [ROOT / "Cargo.toml"]
    for member in members:
        manifest = member / "Cargo.toml"
        data = tomllib.loads(manifest.read_text())
        name = data["package"]["name"]
        names.add(name)
        crates.add(data.get("lib", {}).get("name", name.replace("-", "_")))
        manifests.append(manifest)
    return names, crates, manifests


def compatibility() -> tuple[str, str]:
    _, _, manifests = workspace_packages()
    files = set(manifests)
    files.update(ROOT.glob("**/.cargo/config*"))
    files.update(p for p in (ROOT / "Cargo.lock", ROOT / "rust-toolchain", ROOT / "rust-toolchain.toml", ROOT / ".github/workflows/rust.yml", Path(__file__).resolve(), Path(ledger.__file__).resolve()) if p.is_file())
    digest = hashlib.sha256()
    digest.update(subprocess.check_output(["rustc", "-vV"]))
    variables = {k: v for k, v in sorted(os.environ.items()) if k.startswith(PREFIXES) or k in ("ImageOS", "ImageVersion", "LIBRARY_PATH", "LD_LIBRARY_PATH")}
    digest.update(json.dumps(variables, sort_keys=True).encode())
    for path in sorted(files):
        digest.update(str(path.relative_to(ROOT)).encode())
        digest.update(path.read_bytes())
    family = "hydrus-ws-v1-{}-{}-{}-".format(os.environ["GITHUB_JOB"], os.environ["RUNNER_OS"], os.environ["RUNNER_ARCH"])
    return family, digest.hexdigest()[:24]


def output(name: str, value: object) -> None:
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as file:
        print(f"{name}={value}", file=file)


def key() -> None:
    family, compatible = compatibility()
    restore = family + compatible + "-"
    primary = restore + "{}-{}-{}".format(os.environ["GITHUB_SHA"], os.environ["GITHUB_RUN_ID"], os.environ["GITHUB_RUN_ATTEMPT"])
    output("key", primary)
    output("restore-key", restore)
    print(f"Workspace restore prefix: {restore}")


class UnsafeRestoreError(RuntimeError):
    """Do not run Cargo when rollback or artifact invalidation is uncertain."""


def default_target_only() -> None:
    target = ROOT / "target"
    for name in ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR"):
        value = os.environ.get(name)
        if value and (ROOT / value).resolve() != target.resolve():
            raise ValueError("mtime restore supports only the default CI target")
    for path in (target, target / "debug", target / "debug" / "deps", target / "debug" / "build", target / "debug" / ".fingerprint"):
        if path.is_symlink():
            raise ValueError("symlink-containing default target layout")
    for name in ("CARGO_BUILD_TARGET", "CARGO_BUILD_BUILD_DIR"):
        if os.environ.get(name):
            raise ValueError("mtime restore supports only host-debug/default build-dir")
    configs = list(ROOT.glob("**/.cargo/config*"))
    for parent in ROOT.parents:
        configs.extend(path for path in (parent / ".cargo" / "config", parent / ".cargo" / "config.toml") if path.is_file())
    home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    configs.extend(path for path in (home / "config", home / "config.toml") if path.is_file())
    target_variables = {"CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_TARGET", "CARGO_BUILD_BUILD_DIR"}
    for path in configs:
        config = tomllib.loads(path.read_text())
        if any(option in config.get("build", {}) for option in ("target-dir", "target", "build-dir")):
            raise ValueError("mtime restore does not support Cargo config target/build layout")
        if target_variables.intersection(config.get("env", {})):
            raise ValueError("mtime restore does not support Cargo-config target environment")


def validate_destinations(files: list[Path]) -> None:
    # copy2 follows a pre-existing destination symlink; check every ancestor/file.
    for source in files:
        relative = source.relative_to(SNAPSHOT)
        destination = ROOT
        for component in relative.parts:
            destination = destination / component
            if destination.is_symlink():
                raise ValueError("symlink-containing artifact destination")


def artifact_inventory(files: list[Path], relative_to: Path) -> list[dict]:
    return sorted(({"path": path.relative_to(relative_to).as_posix(), "bytes": path.stat().st_size} for path in files), key=lambda entry: entry["path"])


def invalidate_workspace_fingerprints() -> None:
    # Remove all prior workspace units, including omitted test/bin fingerprints.
    packages, _, _ = workspace_packages()
    fingerprints = ROOT / "target" / "debug" / ".fingerprint"
    if fingerprints.is_symlink():
        raise ValueError("symlink-containing fingerprint root")
    if fingerprints.is_dir():
        for directory in fingerprints.iterdir():
            if any(directory.name.startswith(package + "-") for package in packages):
                if directory.is_symlink():
                    directory.unlink()
                elif directory.is_dir():
                    shutil.rmtree(directory)


def invalidate_workspace_outputs() -> None:
    # Cached units must never pair with residual libraries or build-script outputs.
    invalidate_workspace_fingerprints()
    packages, crates, _ = workspace_packages()
    deps = ROOT / "target" / "debug" / "deps"
    if deps.is_dir():
        for file in deps.iterdir():
            if file.name.endswith(LIB_SUFFIXES) and any(file.name.startswith(crate + "-") or file.name.startswith("lib" + crate + "-") for crate in crates):
                if file.is_file() or file.is_symlink():
                    file.unlink()
    build = ROOT / "target" / "debug" / "build"
    if build.is_dir():
        for directory in build.iterdir():
            if any(directory.name.startswith(package + "-") for package in packages):
                if directory.is_symlink():
                    directory.unlink()
                elif directory.is_dir():
                    shutil.rmtree(directory)


def recover_failed_restore(original_inputs: dict, error: Exception) -> None:
    rollback_errors = ledger.rollback_mtimes(ROOT, original_inputs)
    invalidation_errors = []
    target = ROOT / "target"
    try:
        # A mixed/partial tree must not reach Cargo, even if source rollback worked.
        if target.is_symlink():
            raise ValueError("cannot guarantee target invalidation through symlink")
        if target.exists():
            shutil.rmtree(target)
        if target.exists() or target.is_symlink():
            raise ValueError("target still exists after invalidation")
    except (OSError, ValueError) as failure:
        invalidation_errors.append(str(failure))
    if rollback_errors or invalidation_errors:
        raise UnsafeRestoreError(f"restore failed: {error}; rollback={rollback_errors}; invalidation={invalidation_errors}")
    print(f"Discarded failed workspace restore and restored original input mtimes: {error}; normal cold fallback is safe.")


def restore_snapshot() -> dict | None:
    if SNAPSHOT.is_symlink():
        raise ValueError("symlink-containing workspace snapshot root")
    manifest = SNAPSHOT / "snapshot.json"
    if not manifest.is_file():
        print("No workspace library snapshot restored; dependency cache remains usable.")
        return
    if manifest.is_symlink():
        raise ValueError("symlink-containing workspace metadata")
    metadata = json.loads(manifest.read_text())
    family, compatible = compatibility()
    if metadata.get("schema") != SCHEMA or metadata.get("family") != family or metadata.get("compatibility") != compatible:
        print("Ignoring incompatible workspace library snapshot.")
        return
    staged = SNAPSHOT / "target"
    paths = list(staged.rglob("*")) if staged.is_dir() else []
    files = [p for p in paths if p.is_file() and not p.is_symlink()]
    ledger_path = SNAPSHOT / "inputs.json"
    ledger_size = ledger_path.stat().st_size if ledger_path.is_file() else 0
    if staged.is_symlink() or any(p.is_symlink() or not (p.is_file() or p.is_dir()) for p in paths) or sum(p.stat().st_size for p in files) + ledger_size > MAX_BYTES:
        print("Ignoring oversized or symlink-containing workspace library snapshot.")
        return
    actual_inventory = artifact_inventory(files, SNAPSHOT)
    if metadata.get("inventory") != actual_inventory or metadata.get("files") != len(files) or metadata.get("bytes") != sum(path.stat().st_size for path in files) + ledger_size:
        print("Ignoring incomplete workspace snapshot inventory.")
        return
    cached_inputs = ledger.load(SNAPSHOT / "inputs.json")
    newest_artifact = max((path.stat().st_mtime_ns for path in files), default=0)
    newest_artifact = max(newest_artifact, max(entry["mtime_ns"] for entry in cached_inputs["inputs"]))
    current_inputs, matching = ledger.preflight(ROOT, cached_inputs, newest_artifact)
    default_target_only()
    validate_destinations(files)
    try:
        invalidate_workspace_outputs()
        # Complete the coherent artifact set before changing ANY source timestamp.
        for source in files:
            relative = source.relative_to(SNAPSHOT)
            destination = ROOT / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
        restored_inputs = ledger.stamp_matching(ROOT, cached_inputs, current_inputs, matching)
    except Exception as error:
        recover_failed_restore(current_inputs, error)
        return
    print(f"Verified and restored {restored_inputs} tracked-input mtimes and {len(files)} coherent workspace artifact files; Cargo freshness checks remain enabled.")
    return current_inputs


def restore() -> None:
    # Missing/rejected snapshots safely fall back; uncertain mutations fail closed.
    output("safe", "false")
    original_inputs = None
    try:
        if os.environ.get("WORKSPACE_CACHE_RESTORE_OUTCOME", "success") == "success":
            original_inputs = restore_snapshot()
        else:
            print("Ignoring workspace snapshot because cache extraction did not succeed; normal build fallback.")
    except UnsafeRestoreError as error:
        print(f"UNSAFE workspace restore: {error}")
        raise
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"Ignoring workspace snapshot before mutation: {error}")
    try:
        family, compatible = compatibility()
        baseline = {"family": family, "compatibility": compatible, "inputs": ledger.capture(ROOT)}
        path = ROOT / ledger.BASELINE_NAME
        if path.is_symlink():
            raise ValueError("symlink-containing input baseline")
        path.write_text(json.dumps(baseline, sort_keys=True), encoding="utf-8")
    except Exception as error:
        if original_inputs is not None:
            recover_failed_restore(original_inputs, error)
        # No save/reuse is claimed if the new input baseline could not be recorded.
        raise
    output("safe", "true")


def selected_files() -> list[Path]:
    packages, crates, _ = workspace_packages()
    target = ROOT / "target" / "debug"
    result: set[Path] = set()
    deps = target / "deps"
    if deps.is_dir():
        for file in deps.iterdir():
            if not file.is_file() or file.is_symlink() or not file.name.endswith(LIB_SUFFIXES):
                continue
            if any(file.name.startswith(crate + "-") or file.name.startswith("lib" + crate + "-") for crate in crates):
                result.add(file)
    fingerprints = target / ".fingerprint"
    if fingerprints.is_dir():
        for directory in fingerprints.iterdir():
            if not directory.is_dir() or directory.is_symlink() or not any(directory.name.startswith(package + "-") for package in packages):
                continue
            # Test/binary-only fingerprints are excluded along with their outputs.
            if any(file.name.startswith(("lib-", "build-script-", "run-build-script-")) for file in directory.iterdir()):
                result.update(file for file in directory.rglob("*") if file.is_file() and not file.is_symlink())
    build = target / "build"
    if build.is_dir():
        for directory in build.iterdir():
            if directory.is_dir() and not directory.is_symlink() and any(directory.name.startswith(package + "-") for package in packages):
                result.update(file for file in directory.rglob("*") if file.is_file() and not file.is_symlink())
    return sorted(result)


def stage() -> None:
    output("save", "false")
    if SNAPSHOT.is_symlink():
        print("Skipping workspace snapshot: symlink-containing snapshot root.")
        return
    family, compatible = compatibility()
    baseline_path = ROOT / ledger.BASELINE_NAME
    if baseline_path.is_symlink() or not baseline_path.is_file() or baseline_path.stat().st_size > ledger.MAX_LEDGER_BYTES:
        print("Skipping workspace snapshot: missing/unsafe pre-build input baseline.")
        return
    baseline = json.loads(baseline_path.read_text(encoding="utf-8"))
    current_inputs = ledger.capture(ROOT)
    if baseline.get("family") != family or baseline.get("compatibility") != compatible or not ledger.same_inputs(baseline["inputs"], current_inputs, include_mtime=True):
        print("Skipping workspace snapshot: inputs or compatibility changed during validation.")
        return
    files = selected_files()
    size = sum(file.stat().st_size for file in files) + len(json.dumps(current_inputs, sort_keys=True, indent=2).encode())
    print(f"Workspace library snapshot candidate: {len(files)} artifact files plus input ledger, {size} bytes (limit {MAX_BYTES}).")
    output("bytes", size)
    if not files or size > MAX_BYTES:
        print("Skipping workspace snapshot; dependency caching and validation are unchanged.")
        return
    shutil.rmtree(SNAPSHOT, ignore_errors=True)
    for source in files:
        destination = SNAPSHOT / source.relative_to(ROOT)
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    ledger.write(SNAPSHOT / "inputs.json", current_inputs)
    (SNAPSHOT / "snapshot.json").write_text(json.dumps({"schema": SCHEMA, "family": family, "compatibility": compatible, "source": os.environ["GITHUB_SHA"], "files": len(files), "bytes": size, "inventory": artifact_inventory(files, ROOT)}, indent=2))
    output("save", "true")


if __name__ == "__main__":
    {"key": key, "restore": restore, "stage": stage}[sys.argv[1]]()
