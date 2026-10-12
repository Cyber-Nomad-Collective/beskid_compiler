# Staged OpenSpec change `v06-query-front-end`

OpenSpec lives in the root repository (`openspec/`), outside the compiler repository this
branch belongs to. The coordinator moves this directory to
`openspec/changes/v06-query-front-end/` in the root repository, then runs
`openspec validate v06-query-front-end --strict` and regenerates `openspec/catalog.json`.
