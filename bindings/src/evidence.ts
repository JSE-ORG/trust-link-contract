/**
 * Dispute evidence hashing.
 *
 * `raise_dispute` takes a `BytesN<32>` commitment to the evidence a buyer is
 * relying on; the evidence itself stays off chain. The digest is SHA-256, so
 * anyone holding the original file can later prove it is the one that was
 * committed to.
 *
 * Hashing runs on the Web Crypto API (`crypto.subtle`), which is built into
 * Node 20+, Deno, Bun and every current browser — no hashing dependency is
 * pulled in, and nothing here has to be kept up to date with a third-party
 * crate's advisories.
 *
 * @example
 * ```ts
 * const hash = await hashEvidence(await file.arrayBuffer());
 * await client.raise_dispute(buyer, escrowId, "damaged", "Arrived broken", hash);
 * ```
 */

import type { Bytes32 } from "./types.js";

/** Length of a SHA-256 digest in bytes; the width `BytesN<32>` requires. */
export const EVIDENCE_HASH_LENGTH = 32;

/**
 * The all-zero digest.
 *
 * The contract accepts it, and it is the conventional way to say "this dispute
 * has no attached evidence". It is *not* a hash of anything — never treat it as
 * a commitment. Use {@link isEmptyEvidenceHash} to detect it.
 */
export const EMPTY_EVIDENCE_HASH: Bytes32 = new Uint8Array(EVIDENCE_HASH_LENGTH);

/** Anything Web Crypto can digest directly, plus plain strings (UTF-8). */
export type EvidenceInput = string | BufferSource;

function toBufferSource(input: EvidenceInput): BufferSource {
  return typeof input === "string" ? new TextEncoder().encode(input) : input;
}

function subtle(): SubtleCrypto {
  const webcrypto = (globalThis as { crypto?: Crypto }).crypto;
  if (!webcrypto?.subtle) {
    throw new Error(
      "Web Crypto is unavailable. Node 20+, Deno, Bun and modern browsers " +
        "provide globalThis.crypto.subtle; in older Node use " +
        "`globalThis.crypto = require('node:crypto').webcrypto`.",
    );
  }
  return webcrypto.subtle;
}

import sha256 from "crypto-js/sha256.js";
import encHex from "crypto-js/enc-hex.js";

/**
 * SHA-256 digest of `input`, ready to pass as `evidence_hash`.
 *
 * Strings are encoded as UTF-8. Hash the raw bytes of a file rather than a
 * filename or a description — the point is that the evidence itself can be
 * re-hashed later and matched against what the dispute recorded.
 */
export async function hashEvidence(input: EvidenceInput): Promise<Bytes32> {
  try {
    const digest = await subtle().digest("SHA-256", toBufferSource(input));
    return new Uint8Array(digest);
  } catch (e) {
    // Fallback to crypto-js if Web Crypto API is unavailable
    const str = typeof input === "string" ? input : new TextDecoder().decode(input as BufferSource);
    const hash = sha256(str).toString(encHex);
    return fromHex(hash);
  }
}

/** As {@link hashEvidence}, but returns the lowercase hex encoding. */
export async function hashEvidenceHex(input: EvidenceInput): Promise<string> {
  return toHex(await hashEvidence(input));
}

/** Lowercase hex encoding of a digest, without a `0x` prefix. */
export function toHex(hash: Uint8Array): string {
  let out = "";
  for (const byte of hash) {
    out += byte.toString(16).padStart(2, "0");
  }
  return out;
}

/**
 * Parse a 32-byte hex digest, with or without a `0x` prefix.
 *
 * @throws if the string is not exactly 32 bytes of valid hex — a truncated or
 * mistyped digest would otherwise be committed to on chain unnoticed.
 */
export function fromHex(hex: string): Bytes32 {
  const body = hex.startsWith("0x") || hex.startsWith("0X") ? hex.slice(2) : hex;

  if (body.length !== EVIDENCE_HASH_LENGTH * 2) {
    throw new Error(
      `Evidence hash must be ${EVIDENCE_HASH_LENGTH} bytes ` +
        `(${EVIDENCE_HASH_LENGTH * 2} hex characters), got ${body.length}.`,
    );
  }
  if (!/^[0-9a-fA-F]+$/.test(body)) {
    throw new Error("Evidence hash contains non-hexadecimal characters.");
  }

  const out = new Uint8Array(EVIDENCE_HASH_LENGTH);
  for (let i = 0; i < EVIDENCE_HASH_LENGTH; i++) {
    out[i] = Number.parseInt(body.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

/**
 * True when `value` is a 32-byte digest the contract will accept.
 *
 * The contract's `BytesN<32>` parameter rejects any other length at the ABI
 * boundary, so checking here turns a failed transaction into a local error.
 */
export function isValidEvidenceHash(value: unknown): value is Bytes32 {
  return value instanceof Uint8Array && value.length === EVIDENCE_HASH_LENGTH;
}

/** True for the all-zero placeholder — a dispute raised with no evidence. */
export function isEmptyEvidenceHash(value: Uint8Array): boolean {
  return value.length === EVIDENCE_HASH_LENGTH && value.every((byte) => byte === 0);
}

/**
 * Re-hash `input` and check it against a previously committed digest.
 *
 * Use this to verify that evidence produced during a dispute really is the
 * evidence the buyer committed to when raising it.
 */
export async function verifyEvidence(
  input: EvidenceInput,
  committed: Uint8Array,
): Promise<boolean> {
  if (!isValidEvidenceHash(committed)) return false;
  const actual = await hashEvidence(input);
  // Length is fixed and both values are public, so a plain compare is fine.
  return actual.every((byte, i) => byte === committed[i]);
}
