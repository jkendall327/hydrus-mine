# Re-shown Options wheel source triage

Read-only at `8ada16c768f07d89c676b0ec09a6a56150f26ace`. No approval, build or source edit; frozen source review remains unchanged.

The failed wheel left index 0 instead of the unchanged required 2. Logged geometry was x828/y78/w260/h32, with visible owner, scroll 0 and three choices. The existing saved-policy, owner-valid and allowed assertions passed. The exact dispatch cause remains unproven; this does not demonstrate a product defect.

There is a concrete readiness gap: `settled_options_frame` compares repeated reads of a retained map entry. The Slint geometry Timer is one-shot until a frame change marks measurement pending. Two identical reads therefore do not establish any fresh post-show callback. The earlier repair improved readiness but did not remove this possible cached measurement reuse.

A minimal test-only repair can count geometry callbacks and require a fresh final-size measurement. Clearing the map alone can hang for unchanged geometry; a small actual width transition such as 1101→1100, with real layout/timer pumping and fresh observations after each transition, makes the precondition explicit without setting widget state. Retain the original hidden no-op, exact expected index 2, all prior assertions, two-second bound and single physical enabled wheel. Do not retry input, replace the callback or weaken the expected outcome. Record callback generation, actual adapter size and scroll alongside the existing frame diagnostics. Pointer movement to the measured center can model natural input preparation, but current evidence does not prove that alone repairs this failure.

The sort-selected handler updates synchronously. Merely waiting for the expected result after a missed event is not a source-grounded repair. Full fresh CI must test whichever bounded readiness repair root adopts; no runtime conclusion is granted here.
