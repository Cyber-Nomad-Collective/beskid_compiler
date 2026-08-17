This section documents the **corelib** standard library shipped with the Beskid toolchain. The canonical implementation lives at `compiler/corelib/beskid_corelib/` in the aggregate repo (package identity **`corelib`** on the registry). The library root is **`Prelude.bd`**, which re-exports the modules below so dependents get a single import surface.

## Prelude (re-exported modules)

| Area | Modules |
|------|---------|
| Core | `Core.Results`, `Core.ErrorHandling`, `Core.String`, `Core.Syscall`, stream helpers (`Core.Input`, `Core.Output`, `Core.Error`) |
| Testing | `Testing.Contracts`, `Testing.Assertions` |
| Console | `Console` (`corelib_console`) |

Other modules (`Collections.*`, `Query.*`, `Core.FS`, `Core.Path`, …) ship as separate compilation units under `src/`; import them explicitly when needed— they are not all pulled in through the prelude today.

## Documentation map

- Core: [Core](./Core/README.md)
- Collections: [Collections](./Collections/README.md)
- Query: [Query](./Query/README.md)
- Testing: [Testing](./Testing/README.md)

## Relationship to tooling

- **`beskid pckg pack`** runs doc generation into `.beskid/docs` when packing a project; registry listings include `docs/*.md`, `.beskid/docs/*`, and root `README.md` when present.
- The Starlight **Corelib** section on beskid-lang.org links here via stable GitHub URLs; edit prose in this tree, not only on the website.
