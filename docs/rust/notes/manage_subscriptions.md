# The reference's manage subscriptions dialog

Notes on the reference's code for porting (the old roadmap's item 1, now `docs/rust/history/roadmap-before-2026-10-08.md`).
`oracle/record_subscriptions_list.py` records part of this. Record the
rest before porting it. Quoted strings are verbatim. Paths are under
`hydrus/`.

## Opening, applying, cancelling

`client/gui/ClientGUI.py`.

- **Menu.** network > `subscriptions…`. Its tip is "Change the queries
  you want the client to regularly import from." It sits between
  separators, after the pause submenu (line 3793).
- **The pause submenu** has `subscriptions` (`pause_subs_sync`) and
  `nudge subscriptions awake`.
- **Opening** (`_ManageSubscriptions`, about line 4963):
  1. Pops up 'Waiting for current subscription work to finish.'
  2. Pauses subscriptions for editing and waits until none runs.
  3. Loads them. Missing or surplus query logs ask 'Missing Query Logs!'
     or 'Orphan Query Logs!' (hydrus-rs has no separate log containers,
     so neither applies).
  4. Opens `DialogEdit(self, 'manage subscriptions', ...)` with buttons
     `apply` and `cancel`.
- **Apply** saves the subscriptions ('Saving subscription changes.' in
  a popup) and wakes the subscription manager.
- **Cancel** asks nothing; it just wakes the manager.
- **While a slow job** (loading logs for retry or reset) runs, apply is
  refused: 'It looks like a long-running job is still working. Please
  wait for it to finish before applying this dialog.'

## The list (`EditSubscriptionsPanel`, `client/gui/ClientGUISubscriptions.py:1805`)

**Above the list:**

- When subscriptions are paused from the network menu:
  'SUBSCRIPTIONS ARE CURRENTLY GLOBALLY PAUSED! CHECK THE NETWORK MENU TO
  UNPAUSE THEM.'

**Rows and sorting:**

- Columns and rows: `hydrus-gui-model::subscriptions_list`, recorded.
- The sort is by name at first.
- Double-click and enter edit; the delete key deletes ('Remove all
  selected?').

**Buttons, row 1:**

- `add`:
  - Choose a downloader. With none: 'Hey, you do not have any
    downloaders set up in this client, so you cannot create a new
    subscription yet!\n\nCheck the _network->downloaders_ menu to find
    downloaders made by users.'
  - Then edit `Subscription('new subscription', ...)`.
  - A clashing name becomes 'name (1)', 'name (2)', and so on
    (casefolded).
- `edit`: exactly one selected.
- `delete`: asks 'Remove all selected?'.
- `export`, `import`, `duplicate`: the reference's own serialised
  objects (clipboard, json, png). Probably later, or never.

**Buttons, row 2:**

- `merge`, `separate`, `deduplicate`, `lowercase`: see the code
  (Merge 3393, Separate 3534, DedupeAll 2972, LowerCaseQueries).
- `pause/resume`: toggles each selected.
- **retry** menu, shown where it applies:
  - 'retry ignored' first asks which: 'retry all', 'retry 403s'
    (`^403`), 'retry 404s' (`^404`), 'retry blacklisted'
    (`blacklisted!$`).
  - 'retry failed'.
- `scrub delays`: clears the delay and its reason.
- `check queries now`: asks the "Check which?" prompts (recorded).
- `reset`:
  - Asks 'Resetting these subscriptions will delete all their remembered
    urls, meaning when they next run, they will try to download them all
    over again. This may be expensive in time and data. Only do it if you
    are willing to wait. Do you want to do it?'
  - Each query's last and next check become 0, its status OK, it is
    unpaused, and its file log is emptied.

**Buttons, row 3:**

- `select subscriptions`: by a case-sensitive fragment of a query's
  text (recorded).
- `overwrite downloader`: for all selected; the first selected's
  prefills.
- `overwrite checker options`: the checker options editor, prefilled
  from the first selected, for all selected.

**Buttons, row 4:**

- 'import options:' copy and paste (the paste menu's handlers are wired
  to the wrong functions in the reference).
- favourites.
- `clear` asks 'Clear all custom import options from {names}?'.

## Editing one (`EditSubscriptionPanel`, line 255)

The title is 'edit subscription'. Top to bottom:

1. **Name.** `name: `, a text box.
2. **Delay label.** 'no recent errors', or 'delayed--retrying in 2 hours
   because: reason'. There is no " - " before "because", unlike the
   list's column.
3. **'site and queries' box:**
   - A warning for a multi-site downloader. Its text is in the code,
     about line 300.
   - The downloader button: its name, or 'no downloader set', or
     'not found: name'.
   - The queries list (rows: `query_row`, recorded). Its buttons:
     - `add`:
       - Opens the query editor with the downloader's initial search
         text.
       - A query already there (casefolded) is refused: 'You already
         have a query for "{name}", so nothing new has been added.'
     - `copy queries`: the selected queries' texts, one a line.
     - `paste queries`: see "Pasting queries" below.
     - `edit`:
       - A text that clashes is refused: 'You already have a query for
         "{name}"! The edit you just made will not be saved.'
       - The reference also refuses a change of case only. That is a
         bug; don't copy it.
     - `delete`: asks 'Remove all selected?'.
     - `pause/play`.
     - retry: as the list's.
     - `check now`:
       - Asks the dead and paused queries prompts (`DoAliveOrDeadCheck`
         with no subscriptions).
       - Then sets each chosen query to check now.
       - Clears the subscription's delay.
     - `reset`: 'Resetting these queries will delete all their cached
       urls, meaning when the subscription next runs, they will have to
       download all those links over again. This may be expensive in time
       and data. Only do this if you know what it means. Do you want to do
       it?'
4. **'control' box:** `currently paused: `.
5. **'synchronisation' box:**
   - "Don't change these values unless you know what you are doing!"
   - `on first check, get at most this many urls/posts: ` (1 to 1000;
     50000 in advanced mode).
   - `on normal checks, get at most this many newer urls/posts: `.
   - `do not worry about subscription gaps: `.
   - The `checker options` button.
6. **'file publication' box:**
   - 'If you like, the subscription can send its files to popup buttons
     or pages directly. The files it sends are shaped by the
     'presentation' options in _file import options_.'
   - `show a popup while working: `
   - `publish presented files to a popup button: `
   - `publish presented files to a page: `
   - `publish to a specific label: `: a text box with placeholder
     'subscription files' and a checkbox 'no, use subscription name'.
   - `publish all queries to the same page/popup button: `
7. **The import options button:**
   - 'import options (all default)';
   - 'import options ({type} set)';
   - 'import options ({t1, t2, …})'.

There is no checking on apply.

**A new subscription's defaults:**

- First-check limit 100; normal-check limit 100.
- Checker: 4 files a check, never faster than 1 day, never slower than 90
  days, dead below 1 file in 180 days.
- Show a popup: on. Publish to a popup button: on. Publish to a page:
  off.
- Merge publish events: on.

### Pasting queries (`_PasteQueries`, line 1131)

1. **Reading the clipboard.**
   - Lines are stripped, blank ones dropped.
   - An empty clipboard gives 'The clipboard did not seem to have
     anything in it!'
2. **Sorting the texts.** Each text is compared casefolded with the
   queries there. It is new, already working, or already there but
   DEAD.
3. **The message.**
   - It starts 'I pulled one text from the clipboard. ' or 'I pulled
     {N} texts from the clipboard.\n\n'.
   - Then come blocks:
     - 'They are all new:' or 'These are new:' (or 'It is new:' and
       'This is new:');
     - '... already working in the subscription:';
     - '... already in the subscription but is DEAD:'.
4. **The question.**
   - New and dead: 'Would you like to add the new queries and revive the
     DEAD?'. The yeses are 'do it' and 'add the new, but do not revive
     the DEAD'; the no is 'hold off'.
   - New only: 'Would you like to add these new queries?' ('do it' /
     'hold off').
   - Dead only: 'Would you like to revive these DEAD?'.
   - Nothing to do: 'So there are no actions to take.'

### The query editor (`EditSubscriptionQueryPanel`, line 1584)

The title is 'edit subscription query'.

- **'query and name' box:**
  - `query text: `
  - `optional display name: ` (placeholder 'query name', checkbox 'use
    query text')
- **'status' box:**
  - 'next check: …'
  - `check now: `
  - `paused: `
- **'history' box:** the file log and search log summaries.
- **'tags' box:** additional tags for this query only.
- **'advanced' box** (collapsed): the file log compaction number (100 to
  65536, default 250).

## Time

Every time shown ("1 hour ago", "in 5 days 2 hours", "imminent") is
reckoned from now. Pass `now` in, as `subscriptions_list` does.

- The delay column uses no "now" leeway (`just_now_threshold = 0`).
- The bandwidth waits ("recent delays", and the list's error/delay
  column when not delayed) are not reckoned in hydrus-rs yet; they show
  empty.
