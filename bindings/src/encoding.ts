/**
 * Argument encoding for contract calls.
 *
 * `nativeToScVal` encodes a bare Stellar address string as `ScVal::String`,
 * but the contract's `Address` parameters only accept `ScVal::Address` — a
 * string argument is rejected by the host before the entry point runs. The
 * SDK cannot infer the intent from the value alone, so this module detects
 * address-shaped strings (and the `address` field of a payee object) and
 * encodes them with the `address` type hint.
 *
 * The SDK is loaded lazily by the caller and passed in, so importing this
 * module never pulls `@stellar/stellar-sdk` into a consumer's module graph.
 */

/**
 * The encoding function this module drives. Structurally compatible with the
 * Stellar SDK's `nativeToScVal`, but written so this module does not depend
 * on the SDK's types.
 */
export type Encoder = (value: unknown, opts?: { type?: "address" }) => unknown;

/**
 * Matches a Stellar address: a one-character version byte (`G` account, `C`
 * contract, `M` muxed account) followed by 55 base32 characters.
 *
 * Shape only — the checksum is validated by the SDK when the value is
 * actually encoded, and a value that fails validation falls back to plain
 * string encoding rather than throwing.
 */
const ADDRESS_SHAPE = /^[GCM][A-Z2-7]{55}$/;

/**
 * True when `value` has the shape of a Stellar address.
 *
 * @example
 * ```ts
 * looksLikeAddress("GABC…"); // true
 * looksLikeAddress("TRK-001"); // false
 * ```
 */
export function looksLikeAddress(value: string): boolean {
  return ADDRESS_SHAPE.test(value);
}

/**
 * Encodes a single argument, promoting address-shaped strings to
 * `ScVal::Address` and encoding the `address` field of payee objects.
 *
 * Arrays are encoded element-wise; every other value is passed straight to
 * the encoder.
 */
export function encodeArg<E extends Encoder>(
  value: unknown,
  encode: E,
): ReturnType<E> {
  if (typeof value === "string") {
    if (looksLikeAddress(value)) {
      try {
        return encode(value, { type: "address" }) as ReturnType<E>;
      } catch {
        // Not a valid address after all (bad checksum) — fall back to a
        // plain string rather than failing the whole call.
        return encode(value) as ReturnType<E>;
      }
    }
    return encode(value) as ReturnType<E>;
  }

  if (Array.isArray(value)) {
    return value.map((element) => encodeArg(element, encode)) as ReturnType<E>;
  }

  if (value !== null && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>);
    if (entries.some(([key]) => key === "address")) {
      // Payee-shaped object: `{ address, bps }`. The address field must be
      // an `ScVal::Address` for the contract's `Vec<Payee>` to accept it.
      const encoded: Record<string, unknown> = {};
      for (const [key, entry] of entries) {
        encoded[key] = encodeArg(entry, encode);
      }
      return encode(encoded) as ReturnType<E>;
    }
  }

  return encode(value) as ReturnType<E>;
}

/**
 * Encodes every argument of a contract call, applying {@link encodeArg} to
 * each one.
 */
export function encodeArgs<E extends Encoder>(
  args: readonly unknown[],
  encode: E,
): ReturnType<E>[] {
  return args.map((arg) => encodeArg(arg, encode));
}
