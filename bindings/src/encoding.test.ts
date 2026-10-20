import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { encodeArg, encodeArgs, looksLikeAddress } from "./encoding.js";

/** A syntactically valid Stellar account address (G + 55 base32 chars). */
const ACCOUNT = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
/** A syntactically valid Stellar contract address (C + 55 base32 chars). */
const CONTRACT = "CA3D5KRYM6CB7OWQ6TWYRR3Z4T7GNZLKERYNZGGA5SOAOPIFY6YQGAXE";

describe("looksLikeAddress", () => {
  it("accepts account and contract address shapes", () => {
    assert.equal(looksLikeAddress(ACCOUNT), true);
    assert.equal(looksLikeAddress(CONTRACT), true);
  });

  it("rejects non-address strings", () => {
    assert.equal(looksLikeAddress("TRK-001"), false);
    assert.equal(looksLikeAddress(""), false);
    assert.equal(looksLikeAddress("a".repeat(56)), false);
    // Wrong version byte.
    assert.equal(looksLikeAddress(`S${ACCOUNT.slice(1)}`), false);
  });
});

describe("encodeArg", () => {
  // A stand-in for `nativeToScVal` that records how it was called.
  const fakeEncoder = (calls: { value: unknown; type?: string }[]) =>
    <T>(value: unknown, opts?: { type?: "address" }): T => {
      calls.push({ value, type: opts?.type });
      return value as T;
    };

  it("encodes an address-shaped string with the address type hint", () => {
    const calls: { value: unknown; type?: string }[] = [];
    encodeArg(ACCOUNT, fakeEncoder(calls));
    assert.equal(calls[0].type, "address");
    assert.equal(calls[0].value, ACCOUNT);
  });

  it("leaves ordinary strings untyped", () => {
    const calls: { value: unknown; type?: string }[] = [];
    encodeArg("TRK-001", fakeEncoder(calls));
    assert.equal(calls[0].type, undefined);
  });

  it("falls back to a plain string when the encoder rejects the address", () => {
    const calls: { value: unknown; type?: string }[] = [];
    const throwing = <T>(value: unknown, opts?: { type?: "address" }): T => {
      calls.push({ value, type: opts?.type });
      if (opts?.type === "address") throw new Error("bad checksum");
      return value as T;
    };
    encodeArg(ACCOUNT, throwing);
    assert.equal(calls.length, 2);
    assert.equal(calls[0].type, "address");
    assert.equal(calls[1].type, undefined);
  });

  it("encodes the address field of a payee object with the address type hint", () => {
    const calls: { value: unknown; type?: string }[] = [];
    encodeArg({ address: ACCOUNT, bps: 10_000 }, fakeEncoder(calls));
    // The payee's address is encoded first, then the object itself.
    assert.equal(calls[0].type, "address");
    assert.equal(calls[0].value, ACCOUNT);
    assert.equal(calls[1].type, undefined);
  });

  it("encodes each element of a payee array", () => {
    const calls: { value: unknown; type?: string }[] = [];
    encodeArg(
      [
        { address: ACCOUNT, bps: 7_000 },
        { address: CONTRACT, bps: 3_000 },
      ],
      fakeEncoder(calls),
    );
    assert.deepEqual(
      calls.filter((c) => c.type === "address").map((c) => c.value),
      [ACCOUNT, CONTRACT],
    );
  });

  it("passes plain values straight through", () => {
    const calls: { value: unknown; type?: string }[] = [];
    encodeArg(null, fakeEncoder(calls));
    encodeArg(250, fakeEncoder(calls));
    encodeArg(1_000n, fakeEncoder(calls));
    assert.deepEqual(
      calls.map((c) => c.value),
      [null, 250, 1_000n],
    );
    assert.ok(calls.every((c) => c.type === undefined));
  });
});

describe("encodeArgs", () => {
  it("encodes every argument of a call", () => {
    const calls: { value: unknown; type?: string }[] = [];
    const fakeEncoder = <T>(value: unknown, opts?: { type?: "address" }): T => {
      calls.push({ value, type: opts?.type });
      return value as T;
    };
    encodeArgs([ACCOUNT, null, 250, "notes"], fakeEncoder);
    assert.equal(calls.length, 4);
    assert.equal(calls[0].type, "address");
    assert.equal(calls[1].value, null);
    assert.equal(calls[3].value, "notes");
  });
});
