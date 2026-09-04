# Lumen

A niri-first Wayland shell: bar, launcher, and eventually a control panel and
settings app, built as one coherent product rather than assembled from
separately-themed parts.

Lumen is a fork of [ashell](https://github.com/MalpenZibo/ashell) by
MalpenZibo. That is not a footnote: ashell is the entire foundation here. Its
iced bar, its compositor abstraction, its D-Bus services for network,
bluetooth, upower, tray and mpris, and its module and theming system are what
made starting from scratch pointless. Lumen adds to that work; it does not
replace it.

## License

**GPL-3.0-or-later**, inherited from ashell and not optional. Any code that
lands in this repository is GPL-3.0-or-later, including code written for
Lumen alone.

This is why Colonnade's engine lives outside this repository. See below.

## Relationship to Colonnade

[Colonnade](https://github.com/yesvus/colonnade) is the niri column-grouped
tab strip, currently a GTK3 Waybar module, MIT-licensed. It is not being
moved into Lumen, and it is not being abandoned.

The split, and the reason for it:

- **`colonnade-core` (MIT)** -- the toolkit-independent engine: the niri
  snapshot model, column grouping, tab width maths, the visible-slice anchor
  logic, the marker glyph vocabulary. No GTK, no iced, no rendering at all.
  This is most of what is actually interesting about Colonnade.
- **`colonnade` (MIT)** -- the GTK3 Waybar module, reduced to a renderer over
  the core. Waybar users keep it, standalone, exactly as promised in its own
  README.
- **`lumen` (GPL-3.0-or-later)** -- this repository. A native iced module
  renders the same core.

MIT code may be used in a GPL-3 work; the reverse is not true. Keeping the
engine in its own MIT crate is what lets Colonnade stay a standalone Waybar
module while Lumen consumes the same logic. If the engine lived here, it
would be GPL-3 and Colonnade could never link it back.

## Fork strategy

`main` is Lumen. There is deliberately no local mirror branch of upstream:
`upstream/main` is always one fetch away, and a public repository whose
default branch is unmodified ashell would tell every visitor the wrong
story about what this is.

```bash
git remote -v                     # upstream -> MalpenZibo/ashell
git fetch upstream
git rebase upstream/main          # per upstream release, not per commit
```

Keep changes **additive** wherever possible: new files under `src/modules/`,
new config structs, new variants. ashell dispatches modules through a
`ModuleName` enum matched in `get_module_view` and `get_module_subscription`
rather than a plugin trait, so a new module does touch shared files -- but
only as new match arms, which is the cheapest kind of rebase conflict to
resolve. Resist reformatting, renaming or restructuring upstream code: every
such change is a conflict paid again on every rebase, forever, for no user
benefit.

Features that are not niri-specific should be offered upstream as PRs
instead of carried here. A fork is GPL-3, so that path stays open in both
directions.

## Upstream conventions worth keeping

ashell's own `AGENTS.md` applies to this tree and is worth following even
though this is a fork -- staying close to upstream's conventions is what
keeps rebases cheap:

- `make check` before pushing: format check, `cargo check`, and
  `clippy -D warnings`. Zero warnings.
- Commits: `<type>(<scope>): <subject>`, types `feat|fix|docs|chore|style|refactor|perf|ci`.
- No test suite upstream; quality is enforced by clippy, fmt and build.
