import pg from "pg";

const { Pool } = pg;

let _pool: pg.Pool | undefined;

export function getPool(): pg.Pool {
  if (!_pool) {
    const url = process.env["DATABASE_URL"];
    if (!url) throw new Error("DATABASE_URL environment variable is required");

    _pool = new Pool({
      connectionString: url,
      // pg parses BIGINT (oid 20) as string by default — keep that so BigInt
      // arithmetic in the application layer stays safe.
    });

    _pool.on("error", (err) => {
      console.error("[db] idle client error:", err.message);
    });
  }
  return _pool;
}

export async function closePool(): Promise<void> {
  if (_pool) {
    await _pool.end();
    _pool = undefined;
  }
}

/** Run callback inside a serializable transaction; rolls back on throw. */
export async function withTx<T>(
  pool: pg.Pool,
  fn: (client: pg.PoolClient) => Promise<T>,
): Promise<T> {
  const client = await pool.connect();
  try {
    await client.query("BEGIN");
    const result = await fn(client);
    await client.query("COMMIT");
    return result;
  } catch (err) {
    await client.query("ROLLBACK");
    throw err;
  } finally {
    client.release();
  }
}
