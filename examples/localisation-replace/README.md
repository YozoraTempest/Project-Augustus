<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 Project Augustus contributors -->

# Event Localisation Replacement Fixture

This self-authored Mod queues one AAA event at startup. It demonstrates the event-text loader's supported localisation channels: ordinary English entries and entries under localisation/replace. It does not claim to reproduce all EU4 localisation precedence or duplicate-key behavior.

The ordinary file path sorts after the replacement file path. The ordinary title has two different values, while the replacement title, description, and first option each have one value. The replacement value wins for those keys regardless of the path order. The second option has two different replacement values, so it displays its original key rather than falling through to the ordinary value.

Expected event text:

Title:

~~~text
替换议案
“北方”
~~~

Description:

~~~text
替换正文第一行
替换正文第二行
~~~

First option:

~~~text
替换赞成
立即实施
~~~

Second option fallback:

~~~text
localisation_replace.option_reject
~~~

The external-state file starts AAA at 1500 milli. Choosing the second option adds four treasury units, for 5500 milli.

From the repository root, query the pending event and save the unchanged world:

~~~sh
cargo run --locked -p augustus-run --bin augustus-world -- \
  --game-root examples/world \
  --mod examples/localisation-replace \
  --external-state examples/localisation-replace/external_state.json \
  --player AAA --country AAA --days 0 \
  --vanilla-pending-event-queries examples/localisation-replace/pending-event-queries.json \
  --save localisation-replace.json
~~~

The query is read-only. The pending choice is stored in the snapshot; its presentation text is resolved again from the same content files after restore. Integration tests also generate temporary Mod overlays to verify effective-file shadowing and descriptor replace_path behavior.

The Godot UI integration test checks the rendered strings, snapshot restore, and the second option's treasury effect:

~~~sh
godot --headless --path godot --script res://tests/localisation_replace.gd
~~~

In graphical mode, append -- --capture <absolute-path-prefix> to save a screenshot of the event dialog.
