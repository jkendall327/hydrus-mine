# Runtime-only recorder OUT override; committed recorder is imported unchanged.
import importlib.util, json, sys
from pathlib import Path
repo = Path('/workspace/hydrus-mine')
sys.path.insert(0, str(repo / 'oracle'))
spec = importlib.util.spec_from_file_location('scratch_external_calls', repo / 'oracle/record_external_calls.py')
recorder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recorder)
recorder.OUT = '/workspace/validation-reviews/external-list2/reference/external_calls.actual.json'
sys.argv = [str(repo / 'oracle/record_external_calls.py'), '--child', recorder.OUT]
print('SCRATCH_OUT_OVERRIDE', recorder.OUT, flush=True)
recorder.main()
print('RECORDER_RETURNED_AFTER_NORMAL_CLIENT_SHUTDOWN', flush=True)
