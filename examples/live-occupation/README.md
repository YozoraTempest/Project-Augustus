SPDX-License-Identifier: GPL-3.0-only

Copyright (C) 2026 Project Augustus contributors

# Live Occupation and Monthly Tax

This self-authored native scenario has two fictional countries and one province. AAA owns the province for the entire scenario. Its annual tax attribute has a conditional −50% rate while a known country controller differs from its owner.

The controller starts unknown. The event's immediate effect records AAA as controller. Option 0 changes control to BBB and schedules a return to AAA two days later. Option 1 records known controller absence. Unknown control remains unresolved; known absence takes the non-occupied branch.

```sh
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/live-occupation --input native --trigger live_occupation.tax --save occupation_pending.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/live-occupation --input native --load occupation_pending.json --choose 0:0 --save occupation_bbb.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/live-occupation --input native --load occupation_bbb.json --days 1 --save occupation_december.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/live-occupation --input native --load occupation_december.json --days 31 --save occupation_january.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/live-occupation --input native --load occupation_pending.json --choose 0:1 --save controller_absent.json
cargo run --locked -p augustus-run --bin augustus-events -- --content examples/live-occupation --input native --load controller_absent.json --days 1 --save absence_december.json
```

The first command's event establishes a known unoccupied state. Choosing option 0 makes the December 1 settlement credit AAA 500 milli; the scheduled return takes effect before the January 1 settlement, which credits another 1,000 milli. AAA ends with 1,500 milli, while BBB receives no tax because it never owns the province. Option 1 records a known absence and the December 1 settlement credits AAA the full 1,000 milli.

The CLI test also previews tax before the event: the unknown controller cannot be treated as unoccupied or as a zero modifier, so the account retains an unresolved source and does not credit income. It compares each CLI save with continuous native `EventDemo` execution to cover save/resume determinism. This fixture tests the represented native condition and tax pipeline; it does not claim that all EU4 occupation side effects or tax sources are implemented.

`scene.json` and `events.json` are self-authored JSON data and are distributed under GPL-3.0-only with the copyright notice above.
