#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const requiredLinks = [
  "ARCHITECTURE.md",
  "MULTI_RESOLVER_SCHEME.md",
];
const expectedDestination = path.join(repositoryRoot, "docs", "resolvers.md");
const markdownLink = /\[[^\]]+\]\(([^)\s]+)(?:\s+["'][^)]*["'])?\)/g;

let failed = false;

function error(message) {
  console.error(`❌ ${message}`);
  failed = true;
}

if (!fs.existsSync(expectedDestination)) {
  error("Required destination is missing: docs/resolvers.md");
}

for (const relativeSource of requiredLinks) {
  const source = path.join(repositoryRoot, relativeSource);

  if (!fs.existsSync(source)) {
    error(`Required source is missing: ${relativeSource}`);
    continue;
  }

  const contents = fs.readFileSync(source, "utf8");
  const links = [...contents.matchAll(markdownLink)].map((match) => match[1]);
  const resolverLinks = links.filter((link) => {
    try {
      return path.resolve(path.dirname(source), decodeURI(link)) === expectedDestination;
    } catch {
      return false;
    }
  });

  if (resolverLinks.length === 0) {
    error(`${relativeSource} does not contain a valid link to docs/resolvers.md`);
    continue;
  }

  console.log(`✅ ${relativeSource} -> ${resolverLinks[0]}`);
}

if (failed) {
  process.exitCode = 1;
} else {
  console.log("All required resolver documentation links are valid.");
}
