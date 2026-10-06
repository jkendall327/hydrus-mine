#!/usr/bin/env python3
"""Build once, then replay a selected native test without invoking Cargo again.

The build subcommand belongs outside the workflow's five-minute replay guard.
Only an exact workspace-build artifact may be executed; neither subcommand
searches target directories or accepts a stale executable by filename glob.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
FILTER = 'headless_lifetime::'
BUILD = ['cargo', 'build', '--workspace', '--all-targets', '--locked',
         '--keep-going', '--message-format=json']
RESULT = re.compile(r'^test result: ok\. (\d+) passed; (\d+) failed; '
                    r'\d+ ignored; \d+ measured; \d+ filtered out; finished in .+$')


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    temporary.replace(path)


def verify_source(root, expected):
    if not re.fullmatch(r'[0-9a-f]{40}', expected or ''):
        raise ValueError('Expected a full source SHA')
    actual = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root,
                                     text=True, encoding='utf-8').strip()
    if actual != expected:
        raise ValueError('Source HEAD differs from the expected SHA')
    if subprocess.run(['git', 'diff', '--quiet', 'HEAD', '--'], cwd=root).returncode:
        raise ValueError('Tracked source differs from the source SHA')


def select_artifact(messages, package_id):
    candidates = {}
    for message in messages:
        target = message.get('target', {})
        if (message.get('reason') == 'compiler-artifact'
                and message.get('package_id') == package_id
                and target.get('name') == 'gui' and 'test' in target.get('kind', [])
                and message.get('profile', {}).get('test') is True
                and message.get('executable')):
            candidates[message['executable']] = message
    if len(candidates) != 1:
        raise ValueError(f'Expected one exact GUI integration-test executable; found {len(candidates)}')
    return next(iter(candidates.values()))


def capture_build(root, path):
    """Stream human diagnostics while saving Cargo's unmerged JSON stdout."""
    path.parent.mkdir(parents=True, exist_ok=True)
    messages, malformed = [], []
    with path.open('w', encoding='utf-8') as output:
        with subprocess.Popen(BUILD, cwd=root, stdout=subprocess.PIPE,
                              text=True, encoding='utf-8', errors='replace') as process:
            for line in process.stdout:
                output.write(line)
                output.flush()
                try:
                    message = json.loads(line)
                    if not isinstance(message, dict):
                        raise ValueError('Cargo message is not an object')
                    messages.append(message)
                    if message.get('reason') == 'compiler-message':
                        rendered = message.get('message', {}).get('rendered')
                        if rendered:
                            print(rendered, end='', flush=True)
                except (ValueError, TypeError) as error:
                    malformed.append(str(error))
            code = process.wait()
    # Cargo progress/errors on stderr are inherited by the Actions log. A real
    # Cargo failure wins over JSON validation, retaining its exact exit status.
    if code == 0 and malformed:
        raise ValueError('Malformed Cargo artifact stream: ' + malformed[0])
    if code == 0 and (not messages or messages[-1].get('reason') != 'build-finished'
                      or messages[-1].get('success') is not True):
        raise ValueError('Cargo artifact stream has no successful build-finished record')
    return code, messages


def dll_directories(messages, executable, target_directory, rust_libdir):
    target_directory = Path(target_directory).resolve()
    executable = Path(executable).resolve()
    profile_directory = executable.parent.parent
    if not profile_directory.is_relative_to(target_directory):
        raise ValueError('Executable profile directory is outside the Cargo target directory')
    # Cargo 1.94 Compilation::native_dirs is a BTreeSet. fill_env filters
    # these raw link-search paths under root_output before appending the
    # profile root, deps and sysroot, in that order.
    linked_paths = set()
    for message in messages:
        if message.get('reason') != 'build-script-executed':
            continue
        linked_paths.update(message.get('linked_paths', []))
    directories = []
    for linked in sorted(linked_paths):
        # Cargo JSON link-search values may carry native=/framework= kinds.
        kind, separator, remainder = linked.partition('=')
        path = Path(remainder if separator and kind in {'native', 'framework', 'dependency', 'crate', 'all'}
                    else linked).resolve()
        if path.is_relative_to(profile_directory):
            directories.append(path)
    directories.extend([profile_directory, executable.parent, Path(rust_libdir).resolve()])
    return list(dict.fromkeys(str(path) for path in directories))


def prepare(root, expected, proof_path, messages_path):
    verify_source(root, expected)
    # Replace any previous proof before compilation, including on build failure.
    proof = {'schema': 1, 'source_commit': expected, 'status': 'preparing',
             'build_command': BUILD, 'test_filter': FILTER}
    write_json(proof_path, proof)
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--no-deps', '--format-version=1', '--locked'],
        cwd=root, text=True, encoding='utf-8'))
    packages = [p for p in metadata['packages'] if p['name'] == 'hydrus-gui']
    if len(packages) != 1:
        raise ValueError('Expected one hydrus-gui package in Cargo metadata')
    package = packages[0]
    code, messages = capture_build(root, messages_path)
    proof.update(status='build_failed' if code else 'selecting', build_exit_code=code,
                 cargo_messages_sha256=digest(messages_path))
    write_json(proof_path, proof)
    if code:
        return code
    artifact = select_artifact(messages, package['id'])
    if Path(artifact['manifest_path']).resolve() != Path(package['manifest_path']).resolve():
        raise ValueError('Artifact manifest differs from selected metadata package')
    executable = Path(artifact['executable']).resolve()
    if not executable.is_file():
        raise ValueError('Selected test executable is absent')
    rustc = [os.environ.get('RUSTC', 'rustc'), '--print', 'target-libdir']
    if os.environ.get('CARGO_BUILD_TARGET'):
        rustc += ['--target', os.environ['CARGO_BUILD_TARGET']]
    rust_libdir = subprocess.check_output(rustc, cwd=root, text=True, encoding='utf-8').strip()
    verify_source(root, expected)
    proof.update(status='ready', package_id=package['id'], package_name=package['name'],
                 package_version=package['version'], manifest_path=package['manifest_path'],
                 artifact=artifact, executable=str(executable),
                 executable_sha256=digest(executable),
                 dll_directories=dll_directories(messages, executable,
                                                 metadata['target_directory'], rust_libdir))
    write_json(proof_path, proof)
    print(f'Prepared {executable} ({proof["executable_sha256"]})', flush=True)
    return 0


def validate_proof(root, expected, proof):
    verify_source(root, expected)
    if (proof.get('schema') != 1 or proof.get('status') != 'ready'
            or proof.get('source_commit') != expected or proof.get('build_exit_code') != 0
            or proof.get('build_command') != BUILD or proof.get('test_filter') != FILTER
            or proof.get('package_name') != 'hydrus-gui'):
        raise ValueError('Artifact proof is not a successful exact-source workspace build')
    artifact = select_artifact([proof.get('artifact', {})], proof.get('package_id'))
    executable = Path(proof['executable']).resolve()
    manifest = Path(proof['manifest_path']).resolve()
    if (Path(artifact['executable']).resolve() != executable
            or Path(artifact['manifest_path']).resolve() != manifest
            or not manifest.is_relative_to(root.resolve())):
        raise ValueError('Executable/manifest identity differs from artifact proof')
    if not executable.is_file() or digest(executable) != proof.get('executable_sha256'):
        raise ValueError('Selected executable changed since its workspace build')
    return executable, manifest


def replay(root, expected, proof_path, result_path):
    result = {'schema': 1, 'source_commit': expected, 'status': 'validating',
              'test_filter': FILTER}
    write_json(result_path, result)
    result['artifact_proof_sha256'] = digest(proof_path)
    proof = json.loads(proof_path.read_text(encoding='utf-8'))
    executable, manifest = validate_proof(root, expected, proof)
    env = dict(os.environ)
    # These are Cargo's runtime search directories; never serialize inherited
    # environment values, which may contain credentials.
    env['PATH'] = os.pathsep.join([*proof['dll_directories'], env.get('PATH', '')])
    env['CARGO_MANIFEST_DIR'] = str(manifest.parent)
    env['CARGO_MANIFEST_PATH'] = str(manifest)
    env['CARGO_PKG_NAME'] = proof['package_name']
    env['CARGO_PKG_VERSION'] = proof['package_version']
    command = [str(executable), FILTER, '--nocapture']
    log_path = result_path.with_suffix('.log')
    passed, summaries = 0, 0
    result.update(status='running', command=command, cwd=str(manifest.parent),
                  executable_sha256=proof['executable_sha256'],
                  dll_directories=proof['dll_directories'], log_path=str(log_path))
    write_json(result_path, result)
    # No Cargo/rustc discovery or compilation occurs in this guarded subcommand.
    # The workflow's unchanged five-minute timeout bounds this process tree.
    with log_path.open('w', encoding='utf-8') as output:
        with subprocess.Popen(command, cwd=manifest.parent, env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                              text=True, encoding='utf-8', errors='replace') as process:
            for line in process.stdout:
                print(line, end='', flush=True)
                output.write(line)
                output.flush()
                match = RESULT.fullmatch(line.strip())
                if match and int(match[2]) == 0:
                    summaries += 1
                    passed = int(match[1])
            code = process.wait()
    result.update(status='passed' if code == 0 and summaries == 1 and passed > 0 else 'failed',
                  exit_code=code, passed_test_count=passed, success_summary_count=summaries,
                  test_log_sha256=digest(log_path))
    write_json(result_path, result)
    if code:
        return code
    if result['status'] != 'passed':
        raise ValueError('Replay did not report one successful nonempty filtered test run')
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['build', 'replay'])
    parser.add_argument('--source-sha', required=True)
    parser.add_argument('--proof', type=Path, default=Path('target/ci/windows-gui-artifact.json'))
    parser.add_argument('--messages', type=Path, default=Path('target/ci/windows-build.jsonl'))
    parser.add_argument('--result', type=Path, default=Path('target/ci/windows-gui-replay.json'))
    args = parser.parse_args()
    try:
        if args.command == 'build':
            return prepare(ROOT, args.source_sha, ROOT / args.proof, ROOT / args.messages)
        return replay(ROOT, args.source_sha, ROOT / args.proof, ROOT / args.result)
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as error:
        failure_path = ROOT / (args.proof if args.command == 'build' else args.result)
        try:
            failure = json.loads(failure_path.read_text(encoding='utf-8'))
        except (OSError, ValueError):
            failure = {'schema': 1, 'source_commit': args.source_sha}
        failure.update(status='failed', error=str(error))
        write_json(failure_path, failure)
        print(f'GUI artifact guard: {error}', file=sys.stderr, flush=True)
        return 1


if __name__ == '__main__':
    sys.exit(main())
