# cc-essentials

Rust CLI that runs inside Claude Code as (a) a post-edit formatting hook
and (b) a detection health-check. Purpose: format files Claude just
wrote using the project's biome config, and feed any lint findings back
into Claude's context — without ever blocking a tool execution.

## Commands

- `cc-essentials doctor` — colored health-check report. Shows repo
  root, package manager, biome config + binary + version, cache dir.
- `cc-essentials hooks crite` — reads Claude Code hook JSON from
  stdin, runs `biome check --write --reporter=json` on the touched
  file, writes hook JSON to stdout. Always exits 0.
- `cc-essentials logs` — surfaces the always-on `last-error.json`
  dump and tails `hooks.log` if opt-in logging is enabled. The
  first thing to reach for when a hook misbehaves.

## Stack

- `clap` (derive) — nested subcommand tree
- `serde` + `serde_json` — hook I/O, biome JSON reporter, cache files
- `which` + manual walk-up — binary/config discovery
- `blake3` — cache filename keying
- `dirs` — platform cache dir resolution
- `tempfile::NamedTempFile::persist` — atomic cache writes
- `owo-colors` — conditional ANSI (off when not a TTY / under tests)
- `insta` — snapshot tests for `doctor`
- `assert_cmd` — integration tests for both commands

Binary + library targets (`[lib]` + `[[bin]]`) so integration tests can
import `cc_essentials::...`.

## Module map

```
src/
  main.rs                    → cc_essentials::cli::run()
  lib.rs                     → re-exports
  cli.rs                     → clap tree, dispatch
  commands/
    doctor.rs                → human-readable report
    hooks_crite.rs           → stdin→biome→stdout, always Ok(())
    logs.rs                  → surfaces last-error.json + hooks.log tail
  detect/
    mod.rs                   → DetectedProject + detect_from()
    repo.rs                  → find_git_root
    package_manager.rs       → lockfile priority (bun > pnpm > yarn > npm)
    biome_config.rs          → walk-up biome.json / biome.jsonc
    biome_bin.rs             → node_modules/.bin → $PATH, + version probe
  biome/
    schema.rs                → tolerant deser of --reporter=json output
    run.rs                   → BiomeOutcome, run_check(cwd=config_dir)
    summary.rs               → additional_context (LLM) + system_message (user)
  cache.rs                   → CacheKey, on-disk JSON, atomic rename
  hook_io.rs                 → HookInput / HookOutput (PostToolUse)
  fs_util.rs                 → walk_up_for, FileStamp
  log.rs                     → opt-in JSONL via CC_ESSENTIALS_LOG=1
  error_dump.rs              → always-on last-error.json writer
notes/                       → decision rationale (see below)
tests/
  fixtures/biome_*.json      → canned reporter output (incl. biome_v2_real)
  snapshots/                 → insta snapshots for doctor
  cli_stubs.rs               → smoke tests
  doctor.rs                  → snapshot tests with tempdir scenarios
  hooks_crite.rs             → end-to-end hook contract tests w/ stub biome
  logs.rs                    → logs subcommand integration tests
```

## Detection contract

`detect::detect_from(start, cache) -> Result<DetectedProject>` is the
single entrypoint. Both commands call it. `start` must be a directory
(`hooks_crite` passes `file_path.parent()`). All discovery steps walk
up from `start`; "nearest wins" applies to biome.json, node_modules,
and lockfiles.

`BiomeSetup` is cache-hot — see `src/cache.rs` and `notes/cache-design.md`.

## Hook I/O contract

`hooks crite` writes JSON to stdout per Claude Code's PostToolUse
structured-output schema:

- `hookSpecificOutput.additionalContext` — Claude-facing, only on findings
- `systemMessage` — user-facing terminal line, not seen by Claude
- Empty `{}` — silent no-op (unsupported tool, no biome config, etc.)

See `notes/hooks-contract.md` for the invariants — especially the
always-exit-0 rule.

## Load-bearing invariants

1. **`hooks crite` never exits non-zero.** Tests enforce this across
   every failure path. Don't add `?` at the top of `commands::hooks_crite::run`.
2. **Biome is invoked with cwd = biome.json's parent directory.** Biome
   resolves config from cwd, not from the file path. Monorepos with
   nested biome.json rely on this.
3. **Biome's `check --write` does NOT mutate files with parse errors
   by default.** We depend on this — Claude often writes partial code
   mid-edit. If biome.json turns on `formatter.formatWithErrors`, this
   tool will rewrite broken source. See `notes/biome-caveats.md`.
4. **`last-error.json` is always-on.** `commands::hooks_crite` writes
   it to `<cache_dir>/last-error.json` on every `FallbackText` /
   `SpawnFailed` outcome regardless of `CC_ESSENTIALS_LOG`. This is
   the only evidence users have when the hook silently no-ops — don't
   gate it behind anything.

## Testing

- Unit tests live next to the code they cover (`#[cfg(test)] mod tests`).
- Integration tests in `tests/` use tempdirs + stub biome shell scripts
  that emit canned JSON from `tests/fixtures/`.
- `insta` snapshots for `doctor`: run with `INSTA_UPDATE=always` to
  accept new snapshots, then re-run to confirm stability.
- **No test exercises a real `biome` binary.** See
  `notes/biome-caveats.md` under "No real-biome integration test in CI."

## Common gotchas

- Don't put `start_dir` into the cache key — it causes thrashing in
  monorepos. See `notes/cache-design.md`.
- `--reporter=json` is marked unstable by biome. Our schema types are
  tolerant; don't tighten them without adding version coverage.
- Bin crates have no library by default. `src/lib.rs` exists so
  `tests/*.rs` can `use cc_essentials::...`. When adding a module
  under `src/`, also add a `pub mod` line in `lib.rs` if tests need it.
- `HookInput` must ignore unknown fields — Claude Code's schema adds
  new fields between versions. Don't add `deny_unknown_fields`.
- When running biome, the target file path must be expressed relative
  to `config_dir` (or absolute works too). Never relative to `start`.
- Stub biome scripts in tests must handle `--version` first, because
  detection calls `probe_version` before any `check` invocation.
  Pattern: `if [ "$1" = "--version" ]; then echo 'Version: 1.9.4'; exit 0; fi`
- Biome 2.x emits `diagnostic.message` as a styled segment array, not
  a string, and puts the plaintext in `diagnostic.description`. Our
  `Diagnostic` has a custom `Deserialize` that prefers `description`
  and flattens segments as a fallback. Don't change it to a derived
  `Deserialize` without re-reading `tests/fixtures/biome_v2_real.json`.
- Integration tests MUST override `HOME` (and clear `XDG_CACHE_HOME`)
  before invoking the binary — otherwise they'll write to the
  developer's real cache dir. `tests/hooks_crite.rs::run_hook` already
  does this; copy the pattern when adding new cases.

## Decision notes

Non-obvious architectural choices live under `notes/`:

- [`cache-design.md`](notes/cache-design.md) — OS cache vs project-local,
  mtime+size vs content hash, blake3 filenames, why `start_dir_canonical`
  was removed.
- [`biome-caveats.md`](notes/biome-caveats.md) — reporter instability,
  parse-error behavior, cwd-based config resolution, single-file mode.
- [`hooks-contract.md`](notes/hooks-contract.md) — always-exit-0,
  `systemMessage` vs `additionalContext`, MultiEdit defensive handling.

## Scope

Out of scope for v1 (intentional):

- Windows support. Pass-through might work; we make no guarantees and
  run no CI against it. Tests that require `chmod` are `#[cfg(unix)]`.
- Formatters other than biome. The detection pipeline is biome-shaped.
- Configurable biome flag list. We hardcode `check --write --reporter=json`.
- Cache pruning / GC. Files are ~1KB each and one per biome.json.
- `doctor --json` for machine consumption. Human report only.
