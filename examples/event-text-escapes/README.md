<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# Event Text Escape Fixture

This self-authored Mod queues one AAA event at startup. The title comes from a UTF-8 localisation file with a BOM and contains Chinese text and escaped quotes. The single-line UTF-8 description contains escaped paragraph breaks, a tab, and a doubled backslash before n. A separate Windows-1252 localisation file supplies the first option label; the second option deliberately has no translation and adds three treasury units.

The expected resolved strings are:

Title:

~~~text
议案："北方议事会"
~~~

Description:

~~~text
第一段
第二段	缩进；字面序列 \n 保留
~~~

First option:

~~~text
Café says "oui";	path C:\archive
follow-up
~~~

Second option fallback:

~~~text
event_text_escapes.option_missing
~~~

The external-state file sets AAA treasury to 1200 milli. Choosing the second option raises it to 4200 milli.

From the repository root, query the pending event and save the unchanged world:

~~~sh
cargo run --locked -p augustus-run --bin augustus-world -- \
  --game-root examples/world \
  --mod examples/event-text-escapes \
  --external-state examples/event-text-escapes/external_state.json \
  --country AAA --days 0 \
  --vanilla-pending-event-queries examples/event-text-escapes/pending-event-queries.json \
  --save event-text-escapes.json
~~~

The query is read-only. The pending choice is stored in the snapshot; its presentation text is resolved again from the same content files after restore.

The Godot UI integration test uses this Mod and checks the rendered title, body, both option labels, snapshot restore, and the second option's treasury effect:

~~~sh
godot --headless --path godot --script res://tests/event_text_escapes.gd
~~~

In graphical mode, append -- --capture <absolute-path-prefix> to save a screenshot of the resolved dialog.

The query preserves option tabs as tab characters. Godot displays tabs in button labels as four spaces; title and body labels use tab stops measured from four font spaces.
