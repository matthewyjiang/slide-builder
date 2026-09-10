# Agent permissions

Agent-tool network requests are allowed in Auto, Supervised, and Plan modes,
without an approval prompt. This applies to URL-specific and tool-managed
network requests. The policy does not restrict destinations or distinguish
network reads from writes, so Plan mode is not an offline or read-only network
mode.

Filesystem and process permissions remain separate:

- Auto allows writes and shell commands.
- Supervised allows writes within the deck and cache directories. Other writes
  and shell commands require approval.
- Plan denies writes and shell commands.

Allowing network requests does not register web search or fetch tools. The agent
currently has coding, shell, skill, UI, deck, and asset tools. Shell commands still
follow the process permissions above.

The renderer's network isolation is separate and remains unchanged.
