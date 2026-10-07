# Slint gesture-expiry readiness source review

Read-only assessment while exact `a6d28f4e498a3d7a8f00ad6a61da62166bc54383` CI remains active. No builds, edits or approval; current run must finish unchanged.

The locked Slint core 1.18.1 source confirms a distinct scroll-gesture filter. `api.rs:700–707` maps PointerScrolled to Cancelled phase. `items/flickable.rs:514–522` clamps the new scroll position, clears animation state and records the last scroll event even if no movement occurs. The before-children filter at lines 757–780 can intercept directional wheels at a nearby position while the separately defined 800 ms timeout has not expired. Checking inactive animations therefore does not establish gesture expiry. The runtime timestamp was not logged, so the exact failed dispatch path remains unproven.

A bounded real render/timer pump for at least 810 ms after the final hidden wheel is a source-grounded precondition for the next separate gesture. Keep the actual viewport at 1100×800, the total two-second deadline, visible/current policy checks, stable zero scroll and finite contained stable geometry. No extra wheel, pointer movement, state forcing or resize is needed. Send the original single positive wheel and keep the exact index 2 and hidden no-op assertions.

No new UI hook is needed for this finite case. The existing geometry observer is one-shot until geometry changes: do not clear an unchanged frame map and wait for a callback that cannot occur. Prolonged real rendering drains pending frame-change timers while unchanged measured geometry remains applicable to the same component and unchanged size. A sleep alone without runtime pumping would not demonstrate current_tick expiry.

This would establish recovery after gesture expiry at the stated viewport, not immediate or arbitrary hide/show recovery. The current small-resize readiness change does not prove that broader behavior either. No product defect, completion promotion or runtime sign-off is asserted; any subsequent test repair needs fresh exact-source full Linux execution.
