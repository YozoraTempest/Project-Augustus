# Event Dialog Fixture

This self-authored Mod queues one AAA country event at startup. Its Chinese title and twelve-paragraph description exercise UTF-8 text, using Unicode line separators (U+2028) within a single physical localisation line. Eight long option labels exercise wrapping and scrolling; the final option adds 17 treasury units.

The current loader selects `_l_english.yml` files and preserves their UTF-8 text. Its existing quoted-string decoder removes backslashes before localisation escape handling, so this fixture does not rely on `\n` escapes. Fixing that parsing behavior is separate from the dialog layout.
