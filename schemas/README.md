# knap JSON Schemas

JSON Schema files for knap configuration. Schema versions are independent of
the knap release version — the schema only changes when a breaking change is
made to the configuration interface.

## Current version

`v1` — `schemas/v1/initialization_options.json`

```
https://raw.githubusercontent.com/sleb/knap/main/schemas/v1/initialization_options.json
```

## Versioning rules

A **new schema version** is required when a change would silently break an
existing valid configuration:

| Change                                              | New version?         |
| --------------------------------------------------- | -------------------- |
| Rename an existing key                              | Yes                  |
| Remove an existing key                              | Yes                  |
| Change a key's type                                 | Yes                  |
| Make an optional key required                       | Yes                  |
| Add a new optional key                              | No — update in place |
| Improve a description or default                    | No — update in place |
| Add validation constraints (pattern, minimum, etc.) | No — update in place |

The guiding question: _would an existing user's config stop working or produce
a new validation warning without them changing anything?_ If yes, bump the
version.

## When a new version is needed

1. Create `schemas/vN/initialization_options.json` with the updated schema.
2. Update `docs/GETTING_STARTED.md` to show the new URL and describe what changed.
3. Keep the old schema at `schemas/v(N-1)/initialization_options.json` — existing
   users on the old version should still get valid (if outdated) completions.
4. Add a migration note to `CHANGELOG.md` under the release that introduced the
   breaking change, explaining exactly what to update and why.
5. If a key was renamed, consider adding the old name to the schema as a
   deprecated property with `"description": "Renamed to '…' in schema v2."` to
   surface a hint in the editor.
