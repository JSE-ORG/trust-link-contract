import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCHEMA = (ROOT / "database/postgres/schema.sql").read_text(encoding="utf-8")
COMPOSE = (ROOT / "docker-compose.yml").read_text(encoding="utf-8")
TYPES = (ROOT / "contracts/escrow/src/types.rs").read_text(encoding="utf-8")


def enum_values(sql_type):
    pattern = rf"CREATE TYPE {sql_type} AS ENUM \((.*?)\);"
    match = re.search(pattern, SCHEMA, re.DOTALL)
    if not match:
        return []
    return re.findall(r"'([^']+)'", match.group(1))


def table_body(table_name):
    match = re.search(
        rf"CREATE TABLE IF NOT EXISTS {table_name} \((.*?)\);",
        SCHEMA,
        re.DOTALL,
    )
    if not match:
        return ""
    return match.group(1)


def rust_enum_values(enum_name):
    match = re.search(rf"pub enum {enum_name} \{{(.*?)\n\}}", TYPES, re.DOTALL)
    if not match:
        return []
    values = []
    for line in match.group(1).splitlines():
        value = line.strip().rstrip(",")
        if value and not value.startswith("//"):
            values.append(value)
    return values


class PostgresSchemaTests(unittest.TestCase):
    def test_postgres_container_initializes_schema(self):
        self.assertIn("postgres:", COMPOSE)
        self.assertIn("./database/postgres/schema.sql", COMPOSE)
        self.assertIn("/docker-entrypoint-initdb.d/001_schema.sql:ro", COMPOSE)

    def test_escrow_state_enum_matches_contract_and_keeps_legacy_spelling(self):
        schema_states = enum_values("escrow_state")
        for state in rust_enum_values("EscrowState"):
            self.assertIn(state, schema_states)
        self.assertIn("Cancelled", schema_states)
        self.assertIn("Canceled", schema_states)

    def test_contract_enums_are_available_to_postgres(self):
        self.assertEqual(enum_values("dispute_status"), ["Active", "Resolved"])
        self.assertEqual(enum_values("resolution_type"), ["Release", "Refund"])

    def test_escrows_table_covers_contract_storage_shape(self):
        body = table_body("escrows")
        required_columns = {
            "contract_id": "TEXT NOT NULL",
            "escrow_id": "BIGINT NOT NULL",
            "seller": "TEXT NOT NULL",
            "buyer": "TEXT",
            "resolver": "TEXT NOT NULL",
            "token": "TEXT NOT NULL",
            "amount": "NUMERIC(39, 0) NOT NULL",
            "fee_bps": "INTEGER NOT NULL",
            "resolver_fee_bps": "INTEGER NOT NULL DEFAULT 0",
            "shipping_window": "BIGINT NOT NULL",
            "funded_at": "BIGINT NOT NULL DEFAULT 0",
            "dispute_deadline": "BIGINT NOT NULL DEFAULT 0",
            "state": "escrow_state NOT NULL",
            "shipped_at": "BIGINT NOT NULL DEFAULT 0",
            "delivered_at": "BIGINT",
            "tracking_id": "TEXT",
        }
        for column, definition in required_columns.items():
            self.assertRegex(body, rf"\b{column}\s+{re.escape(definition)}")
        self.assertIn("PRIMARY KEY (contract_id, escrow_id)", body)

    def test_child_tables_reference_escrows_composite_key(self):
        for table in ("escrow_payees", "disputes"):
            body = table_body(table)
            self.assertIn(
                "FOREIGN KEY (contract_id, escrow_id) REFERENCES escrows (contract_id, escrow_id)",
                body,
            )

    def test_schema_has_indexes_for_common_indexer_queries(self):
        for index_name in (
            "idx_escrows_seller",
            "idx_escrows_buyer",
            "idx_escrows_resolver",
            "idx_escrows_token",
            "idx_escrows_state",
            "idx_disputes_status",
            "idx_escrow_events_contract_escrow",
            "idx_escrow_events_topic",
            "idx_escrow_events_ledger",
        ):
            self.assertIn(f"CREATE INDEX IF NOT EXISTS {index_name}", SCHEMA)


if __name__ == "__main__":
    unittest.main()
