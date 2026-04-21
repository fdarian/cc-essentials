# Cache design

## Why the OS cache directory

Cache lives at `<dirs::cache_dir()>/cc-essentials/detect/` (e.g.
`~/Library/Caches/cc-essentials/detect` on macOS).

We chose this over project-local (`.cc-essentials/`) because:

- The cache is disposable, regeneratable, and never meant to be committed.
  Project-local would require a `.gitignore` dance in every consumer repo.
- Detection is fast enough that cache misses are cheap; the cache is a
  latency-of-last-resort, not a correctness mechanism.

## Why mtime + size for staleness (not content hash)

`CacheKey::biome_config_stamp` and `lockfile_stamp` are `FileStamp {
mtime_unix_nanos, size }`. We deliberately do not content-hash the config.

- mtime + size catches every real-world edit — text editors, package
  managers, and CI all touch mtime. Two files with identical content and
  identical mtime+size are functionally identical for our purposes.
- Content hashing would require reading the file on every lookup — cheap
  but not free, and strictly no gain for the common case.
- Known miss: tools that reset mtime (some VCS reflog ops, docker layer
  caches) could theoretically produce stale hits. If this is reported,
  switch to content hash — it's a local change in `fs_util::file_stamp`.

## Why blake3 for the on-disk filename

`entry_path = <cache_dir>/<blake3(abs biome.json path).hex>.json`.

- Stable across process runs (std's `DefaultHasher` is not).
- One file per detected biome.json means no global index, no locking
  complexity, and concurrent hooks on the same project race harmlessly
  (atomic rename, last write wins).
- blake3 is tiny and fast. SHA-2 or xxhash would also work; blake3 wins
  on code size.

## Concurrency: no locks

Two hooks racing on the same project both recompute the same detection
(same inputs → same output), then both write their result to the same
filename. `tempfile::NamedTempFile::persist` does an atomic rename — a
reader either sees the pre-race file or a post-race file, never a partial
one. We accept "both wrote, one won" because the winning payload is valid.

## Invalidation rules

On lookup, we miss if any of these hold:

- File is absent or unreadable.
- Parse fails (corrupt JSON is silently treated as a miss).
- `schema_version != SCHEMA_VERSION` (in-code `const`, bumped on any
  structural change — see c11.5a which bumped to 2).
- Stored `CacheKey` is not byte-for-byte equal to the lookup key.
- `binary_path` no longer exists on disk (biome got uninstalled).
- `config_path` no longer exists (biome.json got moved or deleted).

## Why start_dir_canonical is NOT in the cache key

An earlier iteration included `start_dir_canonical` — the canonicalized
starting path of the detection walk. This was removed in c11.5a because
`BiomeSetup { config_path, binary_path, version }` is fully determined by
the biome.json and the lockfile — neither depends on where the walk
started. Including `start_dir` in the key caused cache thrashing: the
same biome.json reached from `packages/foo` and `packages/bar` produced
different keys that mapped to the same on-disk filename, so each
invocation clobbered the previous.

## No pruning

One cache file per biome.json means the set grows unbounded as you work
on more projects. Each file is ~1KB. We explicitly do not prune in v1 —
if this becomes an issue, a `cc-essentials cache prune` subcommand is
straightforward to add.
