# Prompt composer

The prompt grows with explicit newlines and wrapped input, up to one third of the
terminal height. Input wraps at terminal-cell boundaries without splitting Unicode
graphemes; spaces and blank lines are preserved. Unlike conversation prose, input
can wrap within a word so the editing position stays predictable.

Text, height, and cursor placement share the same layout. An insertion point at
the end of a full row receives a blank continuation row. When input exceeds the
available height, the composer scrolls to keep the cursor within the editable
rows, above the bottom separator. Moving the cursor back reveals earlier lines.
