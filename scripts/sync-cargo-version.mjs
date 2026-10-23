import { readFileSync, writeFileSync } from "node:fs";

const version = JSON.parse(readFileSync("package.json", "utf8")).version;

function replaceOnce(path, pattern, replacement) {
  const source = readFileSync(path, "utf8");
  if (!pattern.test(source)) {
    throw new Error(`${path}: pattern ${pattern} not found`);
  }
  writeFileSync(path, source.replace(pattern, replacement));
}

// Anchored to the [package] table so dependency version keys are untouched.
replaceOnce(
  "Cargo.toml",
  /(\[package\][^[]*?\nversion = ")[^"]*(")/,
  `$1${version}$2`,
);

replaceOnce(
  "Cargo.lock",
  /(\[\[package\]\]\nname = "cc-essentials"\nversion = ")[^"]*(")/,
  `$1${version}$2`,
);
