from pathlib import Path

ROOT = Path("contracts/escrow")
TARGETS = list(ROOT.glob("src/**/*.rs"))
IMPORT_LINE = "use crate::test::single_payee;"

for path in TARGETS:
    if path.name in ("test.rs", "test_helpers.rs"):
        continue
    text = path.read_text()
    if "single_payee(" not in text:
        continue
    if "use crate::test::single_payee" in text:
        continue
    lines = text.split("\n")
    insert_at = 0
    for i, line in enumerate(lines):
        if line.strip().startswith("use "):
            insert_at = i + 1
    lines.insert(insert_at, IMPORT_LINE)
    path.write_text("\n".join(lines))
    print(f"IMPORT ADDED: {path}")