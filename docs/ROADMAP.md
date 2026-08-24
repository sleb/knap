# knap — Roadmap

Each release is designed to be independently useful. A writer should get value
from v0.1 alone and accumulate more with each release.

Released version history lives in [GitHub Releases](https://github.com/sleb/knap/releases)
(changelog) and `docs/design/releases/archive/vX.Y/**/design.md` (the "why"
behind each release). Upcoming work is tracked in
[GitHub Issues](https://github.com/sleb/knap/issues) and sequenced via the
open [`Next` milestone](https://github.com/sleb/knap/milestones). See
`docs/ROADMAP_MIGRATION.md` for how this file got here.

## Principles

- **Each release ships a complete loop.** No half-features that only work after
  the next release.
- **Configuration grows with features.** Don't expose config knobs until the
  feature they control exists.
- **LSP-first.** Avoid editor-specific APIs until a feature genuinely can't be
  expressed in standard LSP.
