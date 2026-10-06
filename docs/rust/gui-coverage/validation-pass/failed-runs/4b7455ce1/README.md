# Failed Linux validation at 4b7455ce1

[Run 37433258092](https://github.com/jkendall327/hydrus-mine/actions/runs/37433258092)
failed one media unit test, `ffmpeg::deadline_tests::version_deadline_is_captured_once_and_explicit_fixed_configuration_wins`,
while waiting for its subprocess PID marker. All 696 GUI tests passed;
strict Clippy and the reference/backend job also passed. This run authorizes
no new completion credit. The signed-off ledger remains 280.

The marker wait hid the worker result, so the saved log does not establish the
underlying subprocess error. Executing a newly written script can race with a
concurrent process inheriting a writable descriptor, but ETXTBSY was not observed
in this log and is not asserted as the proven cause. Ten local repetitions of
the prior 67-test media binary passed.

The test-only repair moves the same authored shell transport into a committed
executable fixture, symlinked into each isolated temporary directory, and reports
premature worker results immediately. It retains real process execution and
FIFO admission, the 1s/2s/20ms/60ms/90ms/3s timing values, kill/reap checks,
version output, deadline capture and explicit-timeout precedence assertions.
Production Ffmpeg code is unchanged. Repaired-source validation remains required.

The full logs and [outcome](validation-outcome.json) preserve the failure.
The [artifact manifest](native-render-manifest.json) binds all 266 native images
and ZIP hash to artifact 11398619802; those failed-run images are diagnostic,
not publication approval. Windows and macOS were deferred.
