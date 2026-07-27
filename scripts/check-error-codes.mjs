#!/usr/bin/env node
/**
 * Drift guard: contracts/escrow/src/errors.rs  <->  bindings/src/errors.ts
 *
 * Fails (exit 1) when the on-chain `ContractError` enum and the TypeScript
 * `ErrorCode` enum disagree on any name or numeric value, or when a code has
 * no entry in ERROR_MESSAGES.
 *
 * Usage:  node scripts/check-error-codes.mjs
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const RUST_FILE = "contracts/escrow/src/errors.rs";
const TS_FILE = "bindings/src/errors.ts";

/** Extract the body of a `{ ... }` block that starts after `header`. */
function blockAfter(source, header, file) {
  const start = source.indexOf(header);
  if (start === -1) throw new Error(`${file}: could not find "${header}"`);
  const open = source.indexOf("{", start);
  if (open === -1) throw new Error(`${file}: no opening brace after "${header}"`);

  let depth = 0;
  for (let i = open; i < source.length; i++) {
    if (source[i] === "{") depth++;
    else if (source[i] === "}") {
      depth--;
      if (depth === 0) return source.slice(open + 1, i);
    }
  }
  throw new Error(`${file}: unbalanced braces after "${header}"`);
}

/** Parse `Name = 12,` variant/member pairs, ignoring comments. */
function parseVariants(body) {
  const out = new Map();
  const withoutComments = body
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/.*$/gm, "");

  for (const line of withoutComments.split("\n")) {
    const m = line.trim().match(/^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(\d+)\s*,?$/);
    if (m) out.set(m[1], Number(m[2]));
  }
  return out;
}

function read(relPath) {
  return readFileSync(join(repoRoot, relPath), "utf8");
}

const rustSource = read(RUST_FILE);
const tsSource = read(TS_FILE);

const rust = parseVariants(blockAfter(rustSource, "pub enum ContractError", RUST_FILE));
const ts = parseVariants(blockAfter(tsSource, "export const enum ErrorCode", TS_FILE));

if (rust.size === 0) throw new Error(`${RUST_FILE}: parsed zero variants`);
if (ts.size === 0) throw new Error(`${TS_FILE}: parsed zero members`);

const errors = [];

for (const [name, value] of rust) {
  if (!ts.has(name)) {
    errors.push(`missing in ${TS_FILE}: ${name} = ${value}`);
  } else if (ts.get(name) !== value) {
    errors.push(`value mismatch for ${name}: ${RUST_FILE}=${value}, ${TS_FILE}=${ts.get(name)}`);
  }
}

for (const [name, value] of ts) {
  if (!rust.has(name)) {
    errors.push(`extra in ${TS_FILE} (not in ${RUST_FILE}): ${name} = ${value}`);
  }
}

// Every code must have a human-readable message.
const messagesBody = blockAfter(tsSource, "export const ERROR_MESSAGES", TS_FILE);
for (const name of rust.keys()) {
  if (!messagesBody.includes(`ErrorCode.${name}`)) {
    errors.push(`ERROR_MESSAGES has no entry for ErrorCode.${name}`);
  }
}

if (errors.length > 0) {
  console.error("ErrorCode drift detected between errors.rs and errors.ts:\n");
  for (const e of errors) console.error(`  - ${e}`);
  console.error(`\n${errors.length} problem(s). Update ${TS_FILE} to match ${RUST_FILE}.`);
  process.exit(1);
}

console.log(`OK — ${rust.size} error codes in sync between errors.rs and errors.ts.`);
