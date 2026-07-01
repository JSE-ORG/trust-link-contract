import type pg from "pg";
import type { Cursor } from "./types.js";

/** Read the persisted cursor (defaults to origin if the row is missing). */
export async function readCursor(client: pg.PoolClient | pg.Pool): Promise<Cursor> {
  const res = await (client as pg.Pool).query<{
    ledger_sequence: string;
    tx_index: number;
    event_index: number;
  }>(
    `SELECT ledger_sequence, tx_index, event_index
       FROM indexer_cursor
      WHERE id = 1`,
  );

  if (res.rowCount === 0) {
    return { ledger_sequence: 0, tx_index: 0, event_index: 0 };
  }

  const row = res.rows[0]!;
  return {
    ledger_sequence: Number(row.ledger_sequence),
    tx_index: row.tx_index,
    event_index: row.event_index,
  };
}

/** Persist the cursor atomically (must be called inside an open transaction). */
export async function writeCursor(client: pg.PoolClient, cursor: Cursor): Promise<void> {
  await client.query(
    `INSERT INTO indexer_cursor (id, ledger_sequence, tx_index, event_index, updated_at)
     VALUES (1, $1, $2, $3, NOW())
     ON CONFLICT (id) DO UPDATE
       SET ledger_sequence = EXCLUDED.ledger_sequence,
           tx_index        = EXCLUDED.tx_index,
           event_index     = EXCLUDED.event_index,
           updated_at      = EXCLUDED.updated_at`,
    [cursor.ledger_sequence, cursor.tx_index, cursor.event_index],
  );
}
