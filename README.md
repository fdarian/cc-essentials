# cc-essentials

A small Rust CLI that makes [Claude Code](https://www.anthropic.com/claude-code)
smarter in JavaScript/TypeScript projects that use
[Biome](https://biomejs.dev/).

Two commands:

- **`doctor`** — a one-shot health check: what repo are we in, what
  package manager, where's the biome config, where's the biome binary,
  what version, where does the cache live.
- **`hooks crite`** (short for "check and write") — designed to run
  inside a Claude Code `PostToolUse` hook. Every time Claude writes or
  edits a JS/TS file, this command formats the file with biome and
  feeds any remaining lint diagnostics back into Claude's context.

## Why

Claude writes partial code mid-edit. Running a formatter automatically
closes the style gap instead of Claude spending tokens on trailing
commas. And surfacing lint findings into Claude's context means
Claude gets a tight review loop without the user having to paste errors
by hand.

Two design decisions matter:

1. **The hook NEVER blocks a tool execution.** It always exits 0. Worst
   case — biome is missing, stdin is garbage, the file doesn't exist —
   the hook is silently a no-op.
2. **Biome won't rewrite files with parse errors.** This is biome's
   default behavior; we rely on it. When Claude writes syntactically
   broken code partway through an edit, the formatter stays out of the
   way.

## Install

From source (rust 1.70+):

```sh
git clone https://github.com/<you>/cc-essentials
cd cc-essentials
cargo install --path .
```

Installs a `cc-essentials` binary to `~/.cargo/bin` (or wherever your
cargo install root is).

## Quick start

### 1. Run `doctor` in your project

```sh
cd path/to/your/ts/project
cc-essentials doctor
```

Example output:

```
cc-essentials doctor
  start: /Users/you/code/my-app
  git repo root: /Users/you/code/my-app (found)
  package manager: Pnpm (/Users/you/code/my-app/pnpm-lock.yaml) (found)
  biome config: /Users/you/code/my-app/biome.json (found)
  biome binary: /Users/you/code/my-app/node_modules/.bin/biome (found)
  biome version: 1.9.4
  cache dir: /Users/you/Library/Caches/cc-essentials/detect
ready: biome detected
```

If any piece is missing, `doctor` tells you — and tells you plainly
when the project isn't supported (no JS/TS lockfile, no biome config).

### 2. Wire up the PostToolUse hook

Add to `.claude/settings.json` (project-local) or `~/.claude/settings.json`
(user-global):

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "cc-essentials hooks crite",
            "timeout": 30,
            "statusMessage": "Running biome..."
          }
        ]
      }
    ]
  }
}
```

That's it. Next time Claude writes or edits a `.ts` / `.tsx` / `.js` /
`.jsx` / `.mjs` / `.cjs` / `.mts` / `.cts` / `.json` / `.jsonc` file in
a project with a `biome.json`, Biome runs, the file is formatted, and
any lint findings flow back into the conversation.

## What Claude sees vs what you see

Claude Code's hook protocol has separate channels. We use both:

- **`systemMessage`** — a terse one-liner in your terminal.
  _Example:_ `cc-essentials: formatted foo.ts (2 warnings)`. Claude
  does not see this.
- **`hookSpecificOutput.additionalContext`** — a structured diagnostic
  summary injected into Claude's context. _Example:_
  ```
  biome report for foo.ts: 0 error(s), 2 warning(s)
    index.ts:1:8 warning(lint/correctness/noUnusedImports): This import is unused.
    index.ts:3:10 warning(lint/suspicious/noExplicitAny): Avoid using any.
  ```
  You don't see this; Claude reads it and can act on it.

On a clean run (no lint findings), Claude's context stays quiet — we
only inject when there's something useful to say.

## Monorepos

`cc-essentials` walks up from the edited file to find the nearest
`biome.json`. If your monorepo has a root config and per-package
configs, each package gets its own — run from `packages/foo/src/x.ts`
uses `packages/foo/biome.json` if present, else falls back to the root.

Biome resolves config from its current working directory, so we set
the subprocess cwd to the biome.json's parent directory and pass the
target file as a relative path. You don't need to configure anything.

## Debugging silent no-ops

Because `hooks crite` is silent on any unsupported input (by design),
use the opt-in log to see what happened:

```sh
CC_ESSENTIALS_LOG=1  # set in your shell environment
# now trigger the hook — Claude edits any file
cat ~/Library/Caches/cc-essentials/detect/hooks.log
```

Each invocation appends one JSONL line: `hook.skip_unsupported_tool`,
`hook.skip_missing_file`, `hook.completed`, etc.

## Caveats

- **Biome's `--reporter=json` flag is marked unstable** by upstream.
  The output shape may change between patch releases. Our schema
  deserialization is tolerant (unknown fields are ignored, missing
  fields default), but a bigger format change would require a version
  bump of `cc-essentials`. If `hooks crite` suddenly goes silent after
  a biome upgrade, run `doctor` first, then file an issue.
- **Unix only for v1** (macOS, Linux). No Windows testing.
- **Biome only for v1.** Prettier / eslint-flat / dprint are not yet
  detected.
- **The cache is unbounded.** One ~1KB JSON file per biome.json you've
  ever hit. Clear it with `rm -rf ~/Library/Caches/cc-essentials` if
  you need to.

## Development

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

Snapshot tests (`tests/doctor.rs`) use `insta`. To accept new
snapshots:

```sh
INSTA_UPDATE=always cargo test --test doctor
```

See `AGENTS.md` for the module map, load-bearing invariants, and
pointers to the `notes/` folder.

## License

MIT
