#!/usr/bin/env node
// Fails if a Tauri npm package and its Rust crate differ in major.minor
// (tauri build refuses to bundle in that case). Reads lockfiles only.
import { readFileSync } from "node:fs";
const root = new URL("../apps/desktop/", import.meta.url);
const lock = JSON.parse(readFileSync(new URL("package-lock.json", root), "utf8"));
const cargo = readFileSync(new URL("src-tauri/Cargo.lock", root), "utf8");
const crates = {};
for (const m of cargo.matchAll(/\[\[package\]\]\nname = "([^"]+)"\nversion = "([^"]+)"/g)) (crates[m[1]] ??= []).push(m[2]);
const mm = (v) => v.split(".").slice(0, 2).join(".");
let bad = 0, checked = 0;
for (const [path, info] of Object.entries(lock.packages ?? {})) {
  const m = path.match(/^node_modules\/@tauri-apps\/(api|plugin-[a-z-]+)$/);
  if (!m) continue;
  const crate = m[1] === "api" ? "tauri" : "tauri-" + m[1];
  const rv = crates[crate];
  if (!rv) continue;
  checked++;
  for (const r of rv) if (mm(r) !== mm(info.version)) { bad++; console.error(`mismatch: ${crate} (${r}) vs @tauri-apps/${m[1]} (${info.version})`); }
}
if (!checked) { console.error("no Tauri pairs found"); process.exit(1); }
if (bad) process.exit(1);
console.log(`Tauri npm/Rust versions aligned (${checked} pairs)`);
