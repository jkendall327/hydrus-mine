# Three validated viewer eye-menu preferences

Validated source `a7ca8166f7779876f7d8167c5db50addef12cbc6`, full Linux run
[37571489508](https://github.com/jkendall327/hydrus-mine/actions/runs/37571489508).
Exactly the browser-viewer Window, Hovers and Rendering collapse preferences
gain sign-off: **354 signed off, 21 existing candidates pending**. All 706
GUI and 67 media tests passed. The two existing tests retain all 22 original
assertions and now contain 49 assertions. This follow-up changes tests and
evidence only; production code and Slint declarations are unchanged.

After Apply, actual pointer input opens the eye menu and refreshes stale flags
to the saved values. All 17 supported checked rows compare against the real Qt
recording. Eight actual root menus, three keyboard-expanded submenus and three
reopened Options states are captured. Actual CheckBox observation covers all
eight combinations. Escape closes the actual popup stack without saving, and
retained predecessor callbacks cannot mutate the successor or its saved state.
Independent and root inspection covered all 14 new defining native images.

The unchanged Qt rerun reproduces all eight menu trees and five supporting
window-action transitions. Its PNG shows Options; the recorder intercepts
PopupMenu to record the actual QAction tree, so there is no displayed Qt popup
image for pixel comparison. Expanded native submenus overlap their parent at the
right edge; their contents remain readable, but the stills do not establish
simultaneous parent highlighting or universal popup placement.

Native fonts, menu layout and headless transport
differ. Existing ICC-profile, duplicate-filter checkerboard, pinned duplicates
hover and Linux warning-label omissions remain. No deeper action, parent, alias,
OS window-manager or direct OS-menu introspection credit is added. Windows and
macOS remain deferred. The optional sandbox-denied Qt API listener is outside
the successful direct Qt/DB rerun and earns no API connectivity claim.

See the exact CI/outcome logs, final independent review, native image manifest,
root rendered review, selected patch and preservation audit in this directory.
Desktop and narrow report checks passed; prior 351 and all unselected assessments
remain intact. The all-leaves goal continues.
