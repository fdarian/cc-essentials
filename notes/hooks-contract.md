# Claude Code hooks contract

`cc-essentials hooks crite` is invoked as a PostToolUse hook. This note
captures the load-bearing invariants of that integration so future
changes don't accidentally break them.

## Invariant: always exit 0

`hooks crite` MUST NEVER exit non-zero. A non-zero exit surfaces a "hook
errored" indicator in the Claude Code transcript, which is visible to
the user and causes confusion. Worse, a bug in the hook that produces
exit 2 could inject spurious "error" context into Claude's conversation.

Enforcement:

- `commands::hooks_crite::run` declares `anyhow::Result<()>` but the
  body unconditionally returns `Ok(())`. The inner `run_inner` is
  allowed to fail; its `Result` is swallowed by `unwrap_or_else(|_|
  HookOutput::default())`.
- If JSON serialization of the output somehow fails, we emit `"{}"` as
  the fallback.
- Integration tests exercise every failure path (empty stdin, garbage
  stdin, unknown tool, missing `file_path`, missing file on disk, no
  biome config, biome fallback-text path) and assert exit 0 in each.

Do NOT add an `?` operator to the body of `run`. If you must add error
handling, do it inside `run_inner`.

## Two output channels, different audiences

Claude Code's PostToolUse JSON output schema supports separate fields
for Claude-facing and user-facing text. We use both:

| Field | Audience | When we set it |
|---|---|---|
| `hookSpecificOutput.additionalContext` | Injected into Claude's context | Only when biome reports diagnostics. Content: one line per diagnostic, errors first. Capped at 50 entries to protect context budget. |
| `systemMessage` | Printed in the user's terminal, NOT visible to Claude | Always when we actually did something (formatted / skipped / biome error). Format: `cc-essentials: formatted foo.ts (N warnings)` |

A clean run (no findings, file formatted cleanly) produces a
`systemMessage` but no `additionalContext` — we don't want to inject
noise into Claude's context on success.

An unsupported input (unknown tool, non-JS extension, missing file_path,
no biome configured) produces an empty `{}` — neither channel fires,
and the user sees nothing. This is intentional: the hook is silent when
it has nothing useful to say.

## Input tolerance

`HookInput` deserialization ignores unknown fields (serde default). We
only read `tool_name`, `tool_input.file_path`, and `cwd`. Everything
else in the hook input payload is discarded.

This matters because Claude Code's schema is unversioned and has added
new fields between versions. Breaking on an unknown field would produce
the one outcome we've sworn off: the hook failing.

## Tool matcher scope

The hook responds to `Write`, `Edit`, and `MultiEdit`. Claude Code's
public docs enumerate Write and Edit but not MultiEdit; its schema is
undocumented. We observe that Claude Code emits `tool_name: "MultiEdit"`
with a `tool_input.file_path` field in practice, so we accept it. If
that ever changes, the defensive `Option<String>` on `file_path` means
we quietly no-op rather than crash.

The recommended `.claude/settings.json` matcher is `"Write|Edit|MultiEdit"`.

## Diagnostics: two files, two audiences

Two artifacts help diagnose hook misbehavior. Both live in
`<cache_dir>/cc-essentials/detect/` and both are surfaced by
`cc-essentials logs`.

### `last-error.json` — always-on, overwritten each time

Written whenever `hooks crite` produces a `BiomeOutcome::FallbackText`
or `BiomeOutcome::SpawnFailed`. No env var required. Contains
everything needed to reproduce: the hook input we read, the exact
`argv` + `cwd` we passed to biome, the biome binary + config paths we
resolved, the exit code, and the first 4KB each of stdout/stderr
(UTF-8-safe truncation).

This is the *load-bearing* invariant of the diagnostic surface: the
next time the hook silently no-ops on someone, the evidence is
already on disk. Do NOT gate this behind `CC_ESSENTIALS_LOG`.

Writes are atomic (`NamedTempFile::persist`) and best-effort — a dump
failure never propagates to the hook output.

### `hooks.log` — opt-in history, JSONL-appended

When `CC_ESSENTIALS_LOG=1` is set in the hook's environment,
`cc-essentials` appends one JSONL line per invocation. Event kinds
include `hook.skip_unsupported_tool`, `hook.completed`,
`hook.stdin_parse_failed`, etc.

On non-`Parsed` outcomes, the `hook.completed` event carries the
first 1KB each of stdout/stderr and the exit code. On `Parsed`
outcomes it stays terse (just `path`, `outcome`,
`has_additional_context`).

Log writes are best-effort. The log file is not rotated or pruned.
If it grows, users can delete it.

### `cc-essentials logs`

One-shot subcommand that reports whether logging is enabled, prints
both paths, pretty-prints `last-error.json` if present, and tails the
last 10 entries of `hooks.log`. This is what users should run first
when the hook misbehaves — faster than grepping the cache dir by
hand.
