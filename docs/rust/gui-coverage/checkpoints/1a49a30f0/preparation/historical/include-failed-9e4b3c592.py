"""Add completed failed-source history to a future gated copy inventory.

This function checks evidence only; it never copies, publishes or grants credit.
Call from the future source helper only after its current CI/render/browser gates.
The prior b3 failure block remains required independently.
"""
from pathlib import Path

FAILED_SOURCE = '9e4b3c59215d017a0a0e0948898901121f0618f5'
FAILED_RUN = 37638222068
FAILED_ARTIFACT = 11492413115
FAILED_ZIP_SHA256 = '2de7ab38bc1f58293f40c7fb97c551e8c1a5491ae2ccde56b769a5d511bab3c1'
DIAGNOSTIC_NAMES = {
    'thumbnail-navigation-options-saved-true.png',
    'thumbnail-navigation-options-saved-false.png',
    'thumbnail-navigation-sidebar-wide.png',
}


def include_failed_history(files, *, base, checkpoint, require, read, sha):
    """Add exactly three diagnostic PNGs and frozen proofs, not all 383 PNGs."""
    base = Path(base)
    failed = base / 'ci-9e4b3c592'
    record = read(failed / 'failed-run-evidence.json')
    require(record['source_commit'] == FAILED_SOURCE and record['run_id'] == FAILED_RUN
            and record['conclusion'] == 'failure' and record['credit_from_this_run'] is False,
            'Completed 9e failed history identity differs')
    require((record['gui_tests']['passed'], record['gui_tests']['failed'],
             record['gui_tests']['ignored']) == (710, 1, 0)
            and (record['media_tests']['passed'], record['media_tests']['failed'],
                 record['media_tests']['ignored']) == (67, 0, 0), 'Failed counts differ')
    collected = {}
    destination = checkpoint + '/failed-runs/9e4b3c592/'
    for name, fingerprint in record['evidence_sha256'].items():
        require(Path(name).name == name, 'Historical log name is not a basename')
        path = failed / name
        require(path.is_file() and not path.is_symlink() and sha(path) == fingerprint,
                'Completed failed evidence bytes differ: ' + name)
        collected[destination + name] = path
    run = read(failed / 'run-latest.json')
    require(run['id'] == FAILED_RUN and run['head_sha'] == FAILED_SOURCE
            and run['status'] == 'completed' and run['conclusion'] == 'failure',
            'Completed hosted run identity differs')
    jobs = read(failed / 'jobs-latest.json')['jobs']
    check = [j for j in jobs if j['name'] == 'check']
    require(len(check) == 1 and check[0]['conclusion'] == 'failure'
            and check[0]['head_sha'] == FAILED_SOURCE and check[0]['run_id'] == FAILED_RUN,
            'Failed check job identity differs')
    clippy = [s for s in check[0]['steps'] if s['name'] == 'clippy']
    require(len(clippy) == 1 and clippy[0]['conclusion'] == 'success',
            '9e Clippy success observation differs')
    manifest = read(failed / 'native-render-manifest.json')
    require(manifest['source_commit'] == FAILED_SOURCE and manifest['run_id'] == FAILED_RUN
            and manifest['artifact_id'] == FAILED_ARTIFACT
            and manifest['zip_sha256'] == FAILED_ZIP_SHA256, 'Failed native identity differs')
    artifact = read(failed / f'artifact-{FAILED_ARTIFACT}.json')
    require(artifact['id'] == FAILED_ARTIFACT and artifact['digest'] == 'sha256:' + FAILED_ZIP_SHA256
            and artifact['workflow_run']['id'] == FAILED_RUN
            and artifact['workflow_run']['head_sha'] == FAILED_SOURCE,
            'Failed artifact metadata differs')
    images = {r['name']: r for r in manifest['images']}
    require(len(images) == record['native_image_count'] == 383
            and 'thumbnail-navigation.png' not in images, 'Missing narrow frame/count differs')
    diagnostic = read(failed / 'root-diagnostic-render-review.json')
    require(diagnostic['approval'] is False and diagnostic['source_commit'] == FAILED_SOURCE
            and diagnostic['run_id'] == FAILED_RUN and diagnostic['artifact_id'] == FAILED_ARTIFACT
            and diagnostic['zip_sha256'] == FAILED_ZIP_SHA256
            and set(diagnostic['missing_required']) == {'thumbnail-navigation.png'}
            and {r['name'] for r in diagnostic['actually_viewed']} == DIAGNOSTIC_NAMES,
            'Failed three-image diagnostic scope differs')
    for frame in diagnostic['actually_viewed']:
        path = failed / 'native-renders' / frame['name']
        require(path.is_file() and not path.is_symlink()
                and sha(path) == frame['sha256'] == images[frame['name']]['sha256']
                and path.stat().st_size == frame['bytes'] == images[frame['name']]['bytes'],
                'Diagnostic image bytes differ: ' + frame['name'])
        collected[destination + 'native/' + frame['name']] = path
    for name in ('failed-run-evidence.json', 'run-latest.json', 'jobs-latest.json',
                 'artifact-11491596725.json', 'artifact-11492413115.json',
                 'artifact-11492947709.json'):
        path = failed / name
        require(path.is_file() and not path.is_symlink(), 'Failed metadata missing: ' + name)
        collected[destination + name] = path
    frozen_review = base / 'thumbnail-navigation3/source-review-2-9e4b3c592.json'
    require(sha(frozen_review) == '2f869fe745e5bc3aac7bc4b97748f0dd0a7dcdbec8d3164df743d58b6fb169ee', 'Frozen 9e source review bytes differ')
    source_packet = read(frozen_review)
    require(source_packet['source_commit'] == FAILED_SOURCE and source_packet['approval'] is False, 'Frozen source-only scope differs')
    for item in source_packet['provenance']:
        path = Path(item['path'])
        require(path.is_relative_to(base) and path.is_file() and not path.is_symlink() and sha(path) == item['sha256'], 'Frozen repair/source provenance differs')
    historical = checkpoint + '/preparation/historical/'
    for name in ('source-review-2-9e4b3c592.json', 'source-review-2-9e4b3c592.md'):
        path = base / 'thumbnail-navigation3' / name
        require(path.is_file() and not path.is_symlink(), 'Frozen 9e source review missing')
        collected[historical + name] = path
    folder = base / 'thumbnail-navigation3/prep-9e4b3c592'
    for name in ('selection.json', 'preparation-summary.json', 'source-preservation-proof.json',
                 'scope-reconciliation-proof.json', 'source-anchor-proof.json',
                 'runtime-repair-source-proof.json', 'conditional-status-map.json', 'README.md'):
        path = folder / name
        require(path.is_file() and not path.is_symlink(), 'Frozen 9e preparation missing')
        collected[historical + 'prep-9e4b3c592/' + name] = path
    for name in ('implementation-proof-3.json', 'expected-captures-3.json', 'diagnosis-and-repair-3.md', 'failed-layout-diagnosis-2.json', 'failed-layout-diagnosis-2.md', 'slint-generation-result.json', 'slint-generation.log', 'generated-content-width-bindings.txt'):
        path = base / 'thumbnail-navigation3/repair1' / name
        require(path.is_file() and not path.is_symlink(), 'Frozen repair1 proof missing')
        destination_key = historical + 'repair1/' + name
        require(destination_key not in collected, 'Repeated historical proof')
        collected[destination_key] = path
    for name, expected in (
        ('diagnostic-render-review-2-9e4b3c592.json', '15b8b06742086681084f65cc5e492d0ded91fa48c89dab7c1f7a63e9173a5ac3'),
        ('diagnostic-render-review-2-9e4b3c592.md', 'dca817a4700ccae4b1c6e35b66877ce04272a3de755dfd45fe5bbc3804a28b71'),
    ):
        path = base / 'thumbnail-navigation3/repair2' / name
        require(path.is_file() and not path.is_symlink() and sha(path) == expected,
                'Frozen independent failed-render review differs: ' + name)
        collected[destination + name] = path
    require(not set(collected) & set(files), 'Historical destination collision')
    files.update(collected)
    return {'source_commit': FAILED_SOURCE, 'run_id': FAILED_RUN, 'conclusion': 'failure',
            'diagnostic_selected_images': 3, 'missing_required_narrow_images': 1,
            'completion_credit': 0, 'not_current_fresh_evidence': True,
            'history_inventory_entries': len(collected)}
