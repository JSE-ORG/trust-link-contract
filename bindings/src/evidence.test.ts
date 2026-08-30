import test from "node:test";
import assert from "node:assert/strict";

import {
  EMPTY_EVIDENCE_HASH,
  EVIDENCE_HASH_LENGTH,
  fromHex,
  hashEvidence,
  hashEvidenceHex,
  isEmptyEvidenceHash,
  isValidEvidenceHash,
  toHex,
  verifyEvidence,
} from "./evidence.js";

// Published SHA-256 vectors, so a broken digest cannot pass by agreeing with
// itself.
const EMPTY_STRING_SHA256 =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const ABC_SHA256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

test("hashEvidence matches the published SHA-256 vector for the empty input", async () => {
  assert.equal(await hashEvidenceHex(""), EMPTY_STRING_SHA256);
});

test("hashEvidence matches the published SHA-256 vector for 'abc'", async () => {
  assert.equal(await hashEvidenceHex("abc"), ABC_SHA256);
});

test("hashEvidence returns exactly 32 bytes", async () => {
  const hash = await hashEvidence("some evidence");
  assert.ok(hash instanceof Uint8Array);
  assert.equal(hash.length, EVIDENCE_HASH_LENGTH);
});

test("hashEvidence treats a string and its UTF-8 bytes identically", async () => {
  const fromString = await hashEvidenceHex("abc");
  const fromBytes = await hashEvidenceHex(new TextEncoder().encode("abc"));
  assert.equal(fromString, fromBytes);
});

test("hashEvidence accepts an ArrayBuffer", async () => {
  const buffer = new TextEncoder().encode("abc").buffer;
  assert.equal(await hashEvidenceHex(buffer as ArrayBuffer), ABC_SHA256);
});

test("hashing is deterministic and input-sensitive", async () => {
  assert.equal(await hashEvidenceHex("receipt.pdf"), await hashEvidenceHex("receipt.pdf"));
  assert.notEqual(await hashEvidenceHex("receipt.pdf"), await hashEvidenceHex("receipt.pdg"));
});

test("toHex and fromHex round-trip", async () => {
  const hash = await hashEvidence("round trip");
  assert.deepEqual(fromHex(toHex(hash)), hash);
});

test("fromHex accepts a 0x prefix", () => {
  assert.deepEqual(fromHex(`0x${ABC_SHA256}`), fromHex(ABC_SHA256));
});

test("fromHex rejects a wrong-length digest", () => {
  assert.throws(() => fromHex(ABC_SHA256.slice(0, 62)), /must be 32 bytes/);
  assert.throws(() => fromHex(`${ABC_SHA256}00`), /must be 32 bytes/);
});

test("fromHex rejects non-hex characters", () => {
  assert.throws(() => fromHex("z".repeat(64)), /non-hexadecimal/);
});

test("isValidEvidenceHash accepts only 32-byte Uint8Arrays", async () => {
  assert.ok(isValidEvidenceHash(await hashEvidence("x")));
  assert.ok(isValidEvidenceHash(EMPTY_EVIDENCE_HASH));
  assert.ok(!isValidEvidenceHash(new Uint8Array(31)));
  assert.ok(!isValidEvidenceHash(new Uint8Array(33)));
  assert.ok(!isValidEvidenceHash(ABC_SHA256));
  assert.ok(!isValidEvidenceHash(null));
});

test("EMPTY_EVIDENCE_HASH is 32 zero bytes and is recognised as empty", () => {
  assert.equal(EMPTY_EVIDENCE_HASH.length, EVIDENCE_HASH_LENGTH);
  assert.ok(EMPTY_EVIDENCE_HASH.every((byte) => byte === 0));
  assert.ok(isEmptyEvidenceHash(EMPTY_EVIDENCE_HASH));
});

test("a real digest is not mistaken for the empty placeholder", async () => {
  assert.ok(!isEmptyEvidenceHash(await hashEvidence("")));
});

test("verifyEvidence confirms matching evidence", async () => {
  const committed = await hashEvidence("original evidence");
  assert.ok(await verifyEvidence("original evidence", committed));
});

test("verifyEvidence rejects tampered evidence", async () => {
  const committed = await hashEvidence("original evidence");
  assert.ok(!(await verifyEvidence("tampered evidence", committed)));
});

test("verifyEvidence rejects a malformed commitment", async () => {
  assert.ok(!(await verifyEvidence("anything", new Uint8Array(31))));
});
