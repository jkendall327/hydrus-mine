All five assigned IDs are eligible for the scoped **Linux first-pass checkpoint** at `56b93ae49337356aa4f81c79b12171397278247b`, run 37425380545 / artifact 11394399260. No canonical changes or publication were performed. Windows/macOS remain deferred; exact-pixel or whole-client Qt parity is not claimed. Fresh GUI suite:696 passed / 0 failed (`linux-full.log:2653`); relevant exact-source test passes are preserved in the JSON report.

Actually viewed and SHA-256 checked against the fresh artifact manifest:

- `gui_format_options.png` — `569608a0db93974d466a51884fd858ca5a2b94f72698bcae3e37fa9ccfc284e9`. 980x850 Options/gui/misc: exact ISO checkbox is visible unchecked, precision spinbox is visible at 6, and Apply/Cancel are visible. This is the last reopened fixture event (input false/7 clamps to false/6), not a factory-default capture. No selected control is clipped or overlapped.
- `popup_job_actions.png` — `1b92dd7373c7d8be727814eab7795663cb71cd38345c79a6dbd5a0b8ba85b671`. 1100x700 Main popup at lower right: live actions title, copy full payload and repeat command buttons are readable and contained. Stop control/summary are visible. Surrounding empty page is test setup, not a reference-layout comparison.
- `popup_job_question.png` — `fbb7ee3db4ceb570656df3eb23394eb8a667639d54697c9edc5dfffaf6d5bfe3`. 1100x700 Main popup: question title, Accept this? body, lowercase yes/no, stop and summary are readable and contained.
- `popup_job_question_dismissed.png` — `9d744f26103ce2cc4f43761020acb4d33c2fef09f5a16db7e1518a0aa2cbb1c0`. 1100x700 Main after accepted answer: popup/question controls are absent; unchanged Main surface remains visible. Native assertion proves actual Store removal and retained producer answer.
- `popup_job_question_fixed32.png` — `33f4f151636c9101061d0d251591a73be2b4afbe8764e9594a7316219f95bc43`. 1280x1200 fixed-32 card: long question wraps to several lines above yes/no; copy payload/run command/stop and summary remain within the card without overlap or lower-edge clipping.
- `popup_job_question_narrow16.png` — `5e60bb6e92041b43c97121a71c07ce8c81a694f2ad4d8c723331e30ec3836291`. 1280x1200 nonfixed-16 card is narrower and taller: full long question wraps, all five measured controls remain contained/readable. Title and action captions fit. Source assertions additionally test the painted stop-button lower border and an actual lower-edge pointer reaching the producer.

GUI formatting (both IDs): the captured reopened controls are exact and unobstructed. `gui_format.rs:112–154` proves draft/Cancel/stale-owner Apply/current Apply/reopen; `:155–267` proves real status/log/download/PNG/parser consumers and Store reopen. Model `gui_format.rs:25–120` replays the eight recorded inputs, including bounds 0/7, byte/time outputs and log columns. The recorder (`record_gui_format.py:62–69`) uses the actual Qt controls and formatters. No matching Qt PNG was identified for this Options/consumer state. The final PNG is false/6, not factory false/3. Existing refresh/UTC/date-boundary/integer-locale caveats remain.

Popup actions (three IDs): clipboard test `popup_job_actions.rs:23–156` checks actual full/replacement clipboard and removal/retirement; callable sections `:77–150,274–398` prove repeated Store effects, current replacement, cancellation, removal and queued/stale GUI retirement; yes/no `:159–269` proves both Boolean replies, immediate real row dismissal and successor/stale-token rejection. Actual handlers/source owner guards and accepted-close composition were read; relevant sources are fingerprinted. Long-card `:401–565` asserts real measured containment, width caps, lower stop-border pixels and an actual pointer reaching the live producer; it supplies extra integration evidence, not new completion credit.

One shared Qt reference was missing from the navigation checklist: **`oracle/fixtures/popup_question_layout.png`**, SHA-256 `6819784937d914cf0f3c3d50bf651a4c8cfdf3df33839e889c8f6a542f9740a0`. I actually viewed it and verified its exact-source bytes. `record_popup_question_layout.py:61–62` records the same 16/nonfixed synthetic question/action state as native narrow16. Labels/order correspond. Qt JSON states `question_word_wrap=false`, its 123×143 card clips the question to one line; native word-wrap makes a taller card and shows the complete long question. This is an explicit presentation difference, not a pixel/layout parity approval. Qt/Slint fonts, character widths, button metrics and host windows also differ. Simple action/short-question/dismissed states still lack matching Qt PNGs.

Eligible IDs:

- `audit-options-gui-misc-experimental-bytes-strings-1kb-pseudo-significant-figures`
- `audit-options-gui-misc-prefer-iso-time-2018-03-01-12-40-23-to-5-days-ago`
- `audit-options-popups-callable`
- `audit-options-popups-clipboard`
- `audit-options-popups-yes-no`

No assigned ID was rejected in this review. Scope remains the two formatting and three action leaves. Cached-label broadcast timing, synchronous Qt callable timing, stock executable producer-family conversion, wider popup/download/freeze/modal/parent scope and Windows/macOS evidence remain excluded.
