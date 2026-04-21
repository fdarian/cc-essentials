# Biome integration caveats

## `--reporter=json` is unstable

Biome's docs and source explicitly mark the JSON reporter as experimental;
the shape may change between patch releases
(https://biomejs.dev/reference/reporters/). Our `biome::schema` types are
deliberately tolerant:

- Every optional field uses `#[serde(default)]`.
- Unknown fields are ignored (default serde behavior, no `deny_unknown_fields`).
- `Location.path` is an untagged enum that accepts either a string or an
  object, because biome has emitted both shapes in different versions.
- On any parse failure, `run_check` returns `BiomeOutcome::FallbackText`
  carrying raw stdout/stderr. Callers downgrade to a best-effort
  `systemMessage` rather than crashing.

If biome ships a stable reporter or renames the flag, update `biome::run`
and the schema types; the rest of the pipeline doesn't care.

## Biome 2.x output shape — what we learned the hard way

The research-phase fixtures we committed (`biome_warnings.json`,
`biome_errors.json`, etc.) used a simplified shape that didn't match
what biome 2.3.11 actually emits. The hook broke in practice the first
time it ran against a real project. Real biome 2.x:

- `diagnostic.message` is an **array of styled segments**
  `[{elements: [], content: "..."}, ...]`, not a plain string.
- `diagnostic.description` is the plaintext form we actually want.
- `location.path` is an object `{file: "/abs/foo.ts"}`, not a bare
  string.
- `location` uses `span: [byte_start, byte_end]` + an attached
  `sourceCode` string, not `start: {line, column}` / `end: {...}`.
- `summary.duration` / `summary.scannerDuration` are
  `{secs, nanos}` objects. We don't deserialize them, so this is
  fine — but worth knowing if you ever add them.

Our handling:

- `Diagnostic` uses a custom `Deserialize` that prefers `description`,
  falling back to concatenating `content` fields from the
  `message` segment array. The legacy string form still parses too.
- `Location` declares both the legacy `start/end` fields and biome
  2.x's `span` + `sourceCode`. All optional.
- `summary::format_loc` uses `line_col` which prefers the legacy
  shape and falls back to walking `sourceCode` to convert
  `span[0]` (byte offset) into `(line, column)`.
- `tests/fixtures/biome_v2_real.json` is captured directly from
  biome 2.3.11 so this exact shape stays locked in.

If biome 3.x changes this again, capture a fresh fixture, update the
schema with new fallbacks, and keep the existing fallbacks — you
don't know which biome version someone's project pins.

## The `--json option is unstable` warning on stderr

Biome 2.x writes this warning to stderr on every `--reporter=json`
invocation. It's noise, not an error, and we filter it out of the
user-facing `systemMessage` when we have to surface stderr content
(the fallback-text path). See `biome::summary::system_message`.

## `check --write` leaves files untouched on parse errors — by default

Per biome source (`FormatWithErrorsDisabled` diagnostic), biome refuses
to format a file it couldn't parse. This is the behavior we depend on:
Claude often writes partial code mid-edit, and we do NOT want the
formatter to rewrite broken source into a different broken state.

Two config flags can change this and should be avoided in biome.json of
projects using this hook:

- `formatter.formatWithErrors: true` — opts in to formatting broken code.
- `--format-with-errors=true` — the CLI equivalent.

We don't set either. The default is safe.

If biome ever changes the default (it hasn't in any 2.x release we
checked), we'd need a pre-pass with `biome check` (no `--write`) to
detect parse errors before deciding to format. We do NOT do that today.

## Config resolution is from cwd, NOT the file path

Biome walks up from the **current working directory** looking for
`biome.json`/`biome.jsonc`, stopping at the first config with
`"root": true`. It does not inspect the target file's path.

Implication for monorepos: if `packages/foo/biome.json` exists alongside
a root `biome.json`, running `biome check packages/foo/bar.ts` from the
repo root will use the root config, not the nested one. Running it from
`packages/foo` uses the nested one.

`hooks_crite::run` handles this by:

1. Walking up from the file's parent directory to find the nearest
   biome.json.
2. Setting the subprocess cwd to that biome.json's parent directory.
3. Passing the file as a path relative to that directory.

See `biome::run::run_check` — `config_dir` is always passed as
`Command::current_dir(...)`.

## Single-file mode bypasses `.gitignore` and `files.includes`

When you pass a specific file path to `biome check <file>`, biome skips
the file-discovery phase — which also means it skips any ignore rules
that would have applied to directory traversal. For our hook that is the
desired behavior: Claude just wrote or edited this specific file; we
want it checked regardless of what the config would exclude during a
full-repo scan.

## Exit codes

Biome exits 0 on clean runs and non-zero when there are lint errors,
parse errors, or CLI problems. We don't branch on biome's exit code —
we parse the JSON and let the structured `summary.errors`/`summary.warnings`
counts drive our summary messages. The exit code is captured in the
`BiomeOutcome::Parsed.exit_code` field in case a future caller needs it.

## No real-biome integration test in CI

All biome invocations in the test suite go through stub shell scripts
that emit canned JSON fixtures from `tests/fixtures/`. We explicitly
chose NOT to require a real biome binary in CI because biome version
drift would flake tests and the contract we care about (argv shape, cwd
behavior, JSON consumption) is exercised by the stubs.

Gap: there is no `#[ignore]`d end-to-end test that shells out to a real
`biome` behind `CC_ESSENTIALS_REAL_BIOME=1`. Add this before a 1.0 tag
that commits to `--reporter=json` stability across biome versions.
