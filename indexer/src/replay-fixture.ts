import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { cursorAfter, type Cursor, type RawEvent } from "./types.js";

export interface ReplayPlan {
  cursor: Cursor;
  startIndex: number;
  pending: RawEvent[];
}

/** Cursor position of one event, for ordering comparisons. */
export function cursorOf(event: RawEvent): Cursor {
  return {
    ledger_sequence: event.ledger_sequence,
    tx_index: event.tx_index,
    event_index: event.event_index,
  };
}

/** Render a cursor as `ledger/tx/event`, the form used throughout the logs. */
export function formatCursor(cursor: Cursor): string {
  return `${cursor.ledger_sequence}/${cursor.tx_index}/${cursor.event_index}`;
}

/**
 * Validate that `value` has the positional fields every event needs.
 *
 * Payload contents are deliberately not checked here. `processEvent` owns
 * that, including the `schema_version` guard.
 */
function assertRawEvent(value: unknown, index: number): asserts value is RawEvent {
  if (value === null || typeof value !== "object") {
    throw new Error(`Fixture entry ${index} is not an object`);
  }
  const event = value as Record<string, unknown>;

  for (const field of ["ledger_sequence", "tx_index", "event_index"] as const) {
    if (!Number.isInteger(event[field])) {
      throw new Error(`Fixture entry ${index} has a non-integer "${field}"`);
    }
  }
  if (typeof event["contract_id"] !== "string") {
    throw new Error(`Fixture entry ${index} has no "contract_id" string`);
  }
  if (!Array.isArray(event["topics"]) || event["topics"].length === 0) {
    throw new Error(`Fixture entry ${index} has no "topics" array`);
  }
  if (event["payload"] === null || typeof event["payload"] !== "object") {
    throw new Error(`Fixture entry ${index} has no "payload" object`);
  }
}

function assertSortedByCursor(events: RawEvent[], fixturePath: string): void {
  for (let i = 1; i < events.length; i++) {
    const prev = cursorOf(events[i - 1]!);
    const curr = cursorOf(events[i]!);
    if (!cursorAfter(curr, prev)) {
      throw new Error(
        `Fixture at ${fixturePath} is not sorted: entry ${i} (${formatCursor(curr)}) ` +
          `does not come after entry ${i - 1} (${formatCursor(prev)}). ` +
          `Events must ascend by (ledger_sequence, tx_index, event_index).`,
      );
    }
  }
}

/**
 * Read and validate a fixture file.
 *
 * @param filePath Path to a JSON array of {@link RawEvent}; relative paths
 *   resolve against the current working directory.
 * @returns The events, in fixture order.
 * @throws If the file is not a JSON array, an entry is missing a positional
 *   field, or the entries are not sorted strictly ascending by cursor.
 */
export function loadFixture(filePath: string): RawEvent[] {
  const abs = resolve(filePath);
  const raw = JSON.parse(readFileSync(abs, "utf-8")) as unknown;

  if (!Array.isArray(raw)) {
    throw new Error(`Fixture at ${abs} must be a JSON array`);
  }

  raw.forEach(assertRawEvent);
  const events = raw as RawEvent[];
  assertSortedByCursor(events, abs);
  return events;
}

/**
 * Index of the first event strictly after `cursor` - where a resumed run picks
 * up.
 */
export function findResumeIndex(events: RawEvent[], cursor: Cursor): number {
  for (let i = 0; i < events.length; i++) {
    if (cursorAfter(cursorOf(events[i]!), cursor)) return i;
  }
  return events.length;
}

export function planReplay(events: RawEvent[], cursor: Cursor): ReplayPlan {
  const startIndex = findResumeIndex(events, cursor);
  return {
    cursor,
    startIndex,
    pending: events.slice(startIndex),
  };
}
