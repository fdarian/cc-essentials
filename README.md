# cc-essentials

> ⚠️ This is all vibecoded. I'm not an experienced in Rust.

A small Rust CLI that makes [Claude Code](https://www.anthropic.com/claude-code)
smarter in JavaScript/TypeScript projects.

Use cases (so far):

- **[Format and lint with Biome](#format-and-lint-with-biome)** — every time
  Claude writes or edits a JS/TS file, format it with [Biome](https://biomejs.dev/)
  and feed any remaining lint diagnostics back into Claude's context.

## Installation

```sh
brew install fdarian/tap/cc-essentials
```

<details>
<summary>From source</summary>

Requires Rust 1.70+:

```sh
git clone https://github.com/fdarian/cc-essentials
cd cc-essentials
cargo install --path .
```

Installs a `cc-essentials` binary to `~/.cargo/bin` (or wherever your
cargo install root is).

</details>

## Use cases

### Format and lint with Biome

Claude writes partial code mid-edit. Running a formatter automatically
closes the style gap instead of Claude spending tokens on trailing
commas. And surfacing lint findings into Claude's context means
Claude gets a tight review loop without the user having to paste errors
by hand.

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

<details>
<summary>What Claude sees vs what you see</summary>

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

</details>

<details>
<summary>Design decisions</summary>

1. **The hook NEVER blocks a tool execution.** It always exits 0. Worst
   case — biome is missing, stdin is garbage, the file doesn't exist —
   the hook is silently a no-op.
2. **Biome won't rewrite files with parse errors.** This is biome's
   default behavior; we rely on it. When Claude writes syntactically
   broken code partway through an edit, the formatter stays out of the
   way.

</details>

<details>
<summary>Caveats</summary>

- **Biome's `--reporter=json` flag is marked unstable** by upstream.
  The output shape may change between patch releases. Our schema
  deserialization is tolerant (unknown fields are ignored, missing
  fields default), but a bigger format change would require a version
  bump of `cc-essentials`. If `hooks crite` suddenly goes silent after
  a biome upgrade, run `doctor` first, then file an issue.
- **Biome only for v1.** Prettier / eslint-flat / dprint are not yet
  detected.

</details>

## Utilities

### `doctor`

A one-shot health check: what repo are we in, what package manager,
where's the biome config, where's the biome binary, what version, where
does the cache live.

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

### `logs`

When a hook silently no-ops, this is the first thing to reach for. It
prints the always-on `last-error.json` dump and tails `hooks.log` if
opt-in logging is enabled (`CC_ESSENTIALS_LOG=1`).

```sh
cc-essentials logs
```

## Caveats

- **Unix only for v1** (macOS, Linux). No Windows testing.
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

Apache 2.0 — see [LICENSE](LICENSE).
