# Conditional next18 + six errata evidence helper

Exact source `7c3c1aac5a60a9e3fb1e7a8864cb439914d4fd60`, Linux run37447108755. Current CI/artifact/reviews/browser gates remain pending; this helper creates no approval. It never writes the repository or runs builds/network requests.

After root composes the gated24-edit packet with `finalize-next18-7c3c1aac5.py`, stages it through the immutable source publication tool and actually inspects/browser-checks the staged report, run:

```sh
python3 /workspace/validation-reviews/prepare-next18-evidence-copy-7c3c1aac5.py
```

Default mode verifies existing evidence and prints its result without writing a copy tree. Required inputs are:

- `publication-next18-7c3c1aac5-linux/{selected-patch.json,review.json,status-reconciliation-audit.json,staged/}`.
- `ci-7c3c1aac5/ci-evidence.json`, full Linux/parity-models logs, and `native-renders/native-render-manifest.json` with all reviewed source-matched PNGs.
- Five `final-review-{2..6}-7c3c1aac5.json/.md` packets approving exactly18 selected claims; reviewer4 additionally approves exactly six prior errata claims with corrected caveats.
- `report-browser-7c3c1aac5/browser-check.json` plus hashed actual desktop/narrow screenshots, matching staged HTML/audit and301+18=319 counts.
- Exact conditional text mapping, original18 subset/preflight, literal six-node erratum/anchor proof and unchanged historical6b review hashes.

After the check succeeds, root may request an atomic scratch mirror:

```sh
python3 /workspace/validation-reviews/prepare-next18-evidence-copy-7c3c1aac5.py --copy-scratch
```

The new output is `publication-next18-7c3c1aac5-linux/copy-ready/`; it must not already exist. The mirror includes reviewable staged payloads, current checkpoint evidence/reviews/referenced native PNGs/logs/browser results, the literal erratum and original historical review/hash proof, helper/preparation provenance and a preservation/copy audit. Failed copying removes the temporary tree. Root separately reviews the mirror, decides canonical integration, updates narrative documents and commits/pushes; this script cannot publish.

Preservation is checked against the exact7c3 `gui_publish`/`gui_coverage` code and immutable inputs:18 new selected IDs,24 reviewed reference IDs, no new native entries, prior301 IDs/statuses/assessments unchanged, prior295 non-anchor objects unchanged. The six exceptions must exactly equal the two reviewed literal limitation corrections plus fresh reviewer4 metadata. All other reference/native differences must equal actual source repin, ancestor aggregate and mapping output; source census and staged input fingerprints must match.

Static preparation checks: AST parses; immutable six-node erratum/explicit authored anchor normalization and original archive/ledger hashes pass. Real default invocation currently blocks on absent CI evidence before creating output. The full successful branch has not been executed during preparation. Any failed/held selected claim, absent approval, mismatched PNG, changed input, wrong browser/counts, or incomplete CI fails closed. A reduced candidate cohort requires a separately reviewed selection/helper change; this helper never auto-approves or silently banks a subset.

The earlier prefetch-value mismatch suspicion was withdrawn after the exact old PNG showed1/1/50; this is historical triage reconciliation only. Current18 eligibility still requires fresh exact-source execution and inspection.

Retained failed115d run37443398472 (695GUI passes,2 failures) remains diagnostic: the scratch mirror includes its root validation-outcome, full/parity/Clippy logs, run/jobs and native-render manifest plus hash-bound failed-run proof under the current checkpoint failed-runs/115d5532a tree. This evidence is never accepted as current successful validation. The narrative erratum copy is versioned `review4-banked-erratum-6b66240c1-at-7c3c1aac5.json`; original115d/6b documents are not overwritten.
