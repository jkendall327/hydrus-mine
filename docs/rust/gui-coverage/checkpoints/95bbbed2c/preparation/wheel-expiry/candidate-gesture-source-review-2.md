# Gesture-expiry fallback source review

Scratch candidate SHA `b0f484aa59919c5eca6e3afdab97eb74c4959fe4025f39314e65db991c06f634`, derived from failed `8ada16c76`. Active `a6d28f4e4` worktree and CI are untouched. No runtime or feature approval.

The candidate preserves the exact prefix before the helper and the complete behavior/input tail from `fn wheel` onward. All 92 assertion spans and six tests remain unchanged. It replaces the two-millisecond minimum with a source-grounded 810 ms real render/timer pump, retains the two-second maximum and original 1100×800 viewport, and adds elapsed/scroll diagnostics. No resize, additional input, widget-state forcing, callback replacement or retry occurs. The single positive wheel and exact expected index 2 remain unchanged.

This is a defensible finite precondition for a distinct gesture after Slint's 800 ms parent scroll filter expires. No new UI hook is needed: unchanged geometry can reuse its existing measurement while prolonged real pumping drains any pending changed-frame Timer. It does not claim a fresh callback when no frame change occurs or prove immediate hide/show recovery. The internal timestamp is not directly observed; ordinary runtime clock progression and final actual input/result checks remain necessary.

No concrete source/API/sequencing blocker was found. Adoption remains conditional on the current run outcome and requires exact-source execution. Frozen source/repair packets are unchanged.
