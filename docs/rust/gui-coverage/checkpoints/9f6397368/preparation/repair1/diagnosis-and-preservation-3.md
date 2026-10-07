# Sidebar setup repair 1

Commit `b51511a01221f04a8b88808c9e4c558d0b436fed` changes only `crates/hydrus-gui/tests/gui/sidebar_context_cogs.rs`: five inserted lines, no deletions. No builds or runtime tests were run.

The failed source was `393c125530d004153a92ed79e51da91539df8837`, run 37574215162. Its log at `full-check.log:2670–2685` shows the new test failing at old line 599. Parsed log values prove the left side equals all 30 incoming `fixture.files` exactly; the right side equals the my-tags, namespace mode-2 recorded sorted result exactly. Membership matches; ordering differs.

`SearchPage::restored` (`page.rs:349–380`) stores the incoming vector without resorting. The original native replay runs `order_chosen(0)` before its comparisons (`sidebar_context_cogs.rs:200–212`). The actual Qt recorder runs `SetSort` and recorded service/display activation before mouse routes (`record_sidebar_sort_collect_cogs.py:62–100`). The setup therefore needed the existing ordinary consumer before claiming a sorted no-op baseline.

The repair invokes the real `ui.invoke_order_chosen(0)` before the initial right-click. Production `lib.rs:1759–1774` sets ascending order, resorts and refreshes through that route. The fixture is explicitly ascending (`order:0`) and save-on-change is off. A new complete sorted-media assertion establishes the exact baseline **before** right-click; the original failing full equality remains **after** it. No expected value or comparison was weakened.

All previous file lines are preserved in order. All 87 current assertions remain, with one new baseline assertion (88 total across three tests). The original banked 15,749-byte prefix remains exact, including its 25 assertions, two tests and full 16-sort/8-collect replay. The four defining capture names and all retirement/save-policy assertions are unchanged. Prior implementation proof files remain frozen.

Rustfmt check (edition 2024) and `git diff --check` passed. Independent reviewer 2 found no source blocker. Root still needs an exact integrated-source full Linux rerun and fresh renders; this source repair is not runtime approval. The fixture's equal media order across display modes remains an explicit limit.
