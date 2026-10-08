# External callables: report

## Leaves tagged

| Leaf | Test |
|---|---|
| audit-options-external-programs-help-for-this-panel | `external_calls_editor::help_button_opens_the_external_programs_documentation` |
| audit-options-nested-external-call-input-rules | `external_calls_editor::input_rules_enable_token_and_string_processor_reach_the_saved_call` |
| audit-options-nested-external-call-process-timeout | `external_calls_editor::timeout_long_lived_terminal_and_text_flags_are_saved_on_the_process` |
| audit-options-nested-external-call-command-path | `external_calls_editor::executable_path_checks_and_show_path_help_match_the_reference` |
| audit-options-nested-external-call-test-availability | `external_calls_editor::availability_test_is_a_which_call_with_the_reference_messages` |
| audit-options-external-programs-external-calls-add | `external_calls_editor::add_creates_a_new_call_and_export_writes_...` |
| audit-options-external-programs-external-calls-export | same test (export compared with `oracle/fixtures/external_calls.json`, keys blanked) |
| audit-options-external-programs-external-calls-edit | `external_calls::callable_command_child_apply_cancel_...` (existing, tagged) |
| audit-options-external-programs-external-calls-import | `external_calls::imported_reference_calls_review_...` (existing, tagged) |
| audit-options-nested-external-call-test-execution | `external_calls::reopened_saved_process_uses_real_editor_inputs_...` (existing, tagged) |

Most of the feature already existed; the leaves were stale "missing".

## Implemented / fixed

- Options page: reference intro text and a help menu button ("open the external
  programs help" opens `external_programs.html`).
- Editor: job summary (Summary / Available input parameters / Expected output
  parameters), "choose the type of call", process-call text with the
  "availability is a which call" note, platform default-launch wording, the
  sandbox hint, the command-window paste hint, and the full PATH warning.
- Bug: the rule's string processor window was dropped as it opened; it is now
  kept in its slot (inspectable and cancellable).

## Remains

- "send multiple files" job (not ported; in DIFFERENCES.md).
- Timeout widget is seconds-only; no tooltips; help is a "help ▾" button.
- I did not compare the new static Slint strings by rendering them against Qt;
  only properties and behaviour are asserted. The unchanged geometry test for
  "add defaults" still passes (the intro text did not move the button out of
  its pinned hit area once the duplicate heading was removed).

## Workflow friction

- First build on this machine (`scripts/dev.sh ui`): 22m43s, including the
  rustup toolchain install. A concurrent `dev.sh slint` waited on Cargo's lock
  behind it, so the first Slint error only appeared ~20 minutes after the edit.
- Each Slint edit then rebuilt the generated UI crate before tests ran
  (about 7 minutes, as documented).
- Typical edit-test loop after that: `dev.sh gui external_calls` ~20-40 s for a
  Rust/test edit, ~10 s of that running the 16 tests.
- A pre-existing geometry test (`defaults_physical_menu...`) hard-codes the
  Add Defaults button position, so adding text above the table breaks it with
  no hint why; a probe screenshot was the quickest way to see the shift.
