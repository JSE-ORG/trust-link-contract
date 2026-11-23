#!/usr/bin/env python3
"""Gas-regression gate for the escrow contract (issue #1323).

Runs the `test_gas_profile` unit tests, parses the `gas_profile | ...` metric
lines, and compares CPU instructions and memory bytes against a committed
baseline. Any metric that grows by more than the allowed threshold (default
5%) fails the command, which is what CI runs via `make bench-gas`.

Usage:
    python3 scripts/check_gas_regression.py            # check
    python3 scripts/check_gas_regression.py --update   # rewrite baseline
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

DEFAULT_BASELINE = Path("contracts/escrow/gas_baseline.json")
GAS_LINE = re.compile(
    r"gas_profile\s*\|\s*(?P<label>.+?)\s*\|\s*cpu=\s*(?P<cpu>\d+)\s*\|\s*mem=\s*(?P<mem>\d+)"
)


def parse_metrics(stdout: str) -> dict[str, dict[str, int]]:
    metrics: dict[str, dict[str, int]] = {}
    for line in stdout.splitlines():
        match = GAS_LINE.search(line)
        if not match:
            continue
        metrics[match.group("label")] = {
            "cpu_insns": int(match.group("cpu")),
            "mem_bytes": int(match.group("mem")),
        }
    return metrics


def run_gas_tests() -> dict[str, dict[str, int]]:
    command = [
        "cargo",
        "test",
        "--lib",
        "test_gas_profile",
        "--",
        "--nocapture",
        "--test-threads=1",
    ]
    print("==> Running:", " ".join(command), flush=True)
    completed = subprocess.run(command, capture_output=True, text=True)
    stdout = completed.stdout + completed.stderr
    metrics = parse_metrics(stdout)

    print("\n=== Gas profile samples ===")
    for label in sorted(metrics):
        sample = metrics[label]
        print(
            f"  {label:<40} cpu={sample['cpu_insns']:>12}  mem={sample['mem_bytes']:>10}"
        )

    if completed.returncode != 0:
        print("\nGas-profile tests failed:", file=sys.stderr)
        print(stdout[-4000:], file=sys.stderr)
        raise SystemExit(completed.returncode)
    if not metrics:
        print(
            "\nNo gas_profile metrics were emitted; refusing to pass a gate that measured nothing.",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return metrics


def load_baseline(path: Path) -> dict[str, dict[str, int]]:
    try:
        raw = json.loads(path.read_text())
    except FileNotFoundError:
        print(f"Baseline not found: {path} (run with --update)", file=sys.stderr)
        raise SystemExit(1)
    return {
        label: {"cpu_insns": int(values["cpu_insns"]), "mem_bytes": int(values["mem_bytes"])}
        for label, values in raw.items()
        if not label.startswith("_")
    }


def write_baseline(path: Path, metrics: dict[str, dict[str, int]]) -> None:
    payload = {
        "_comment": (
            "Gas baselines for contracts/escrow/src/test_gas_profile.rs. "
            "Regenerate with `make bench-gas-update` after an intentional change."
        ),
        **{label: metrics[label] for label in sorted(metrics)},
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=False) + "\n")
    print(f"\nWrote {len(metrics)} baselines to {path}")


def allowed_limit(base: int, threshold: float) -> int:
    # Integer ceiling so a tiny baseline still gets at least one unit of slack.
    return base + max(1, int(base * threshold))


def check(metrics: dict[str, dict[str, int]], baseline: dict[str, dict[str, int]], threshold: float) -> bool:
    failures: list[str] = []
    observed = set(metrics)
    expected = set(baseline)

    for label in sorted(expected - observed):
        failures.append(f"{label}: missing from this run (test removed or renamed)")
    for label in sorted(observed - expected):
        failures.append(f"{label}: no baseline entry (run `make bench-gas-update`)")

    print("\n=== Gas regression check (threshold {:.0f}%) ===".format(threshold * 100))
    print(f"{'Operation':<40} {'cpu base':>12} {'cpu now':>12} {'delta':>9} {'mem base':>10} {'mem now':>10} {'delta':>9}")

    for label in sorted(observed & expected):
        base = baseline[label]
        now = metrics[label]
        cpu_limit = allowed_limit(base["cpu_insns"], threshold)
        mem_limit = allowed_limit(base["mem_bytes"], threshold)

        cpu_delta = percent_delta(base["cpu_insns"], now["cpu_insns"])
        mem_delta = percent_delta(base["mem_bytes"], now["mem_bytes"])
        print(
            f"{label:<40} {base['cpu_insns']:>12} {now['cpu_insns']:>12} {cpu_delta:>8.2f}% "
            f"{base['mem_bytes']:>10} {now['mem_bytes']:>10} {mem_delta:>8.2f}%"
        )

        if now["cpu_insns"] > cpu_limit:
            failures.append(
                f"{label}: cpu {now['cpu_insns']} exceeds {cpu_limit} "
                f"(baseline {base['cpu_insns']}, {cpu_delta:+.2f}%)"
            )
        if now["mem_bytes"] > mem_limit:
            failures.append(
                f"{label}: mem {now['mem_bytes']} exceeds {mem_limit} "
                f"(baseline {base['mem_bytes']}, {mem_delta:+.2f}%)"
            )

    if failures:
        print("\nGas regression detected:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        print(
            "\nIf the increase is intentional and justified, regenerate the baseline with "
            "`make bench-gas-update` and commit the new snapshot.",
            file=sys.stderr,
        )
        return False

    print(f"\nPASS: {len(observed)} operations within {threshold * 100:.0f}% of baseline.")
    return True


def percent_delta(base: int, current: int) -> float:
    if base == 0:
        return 0.0 if current == 0 else float("inf")
    return (current - base) / base * 100.0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline",
        type=Path,
        default=DEFAULT_BASELINE,
        help=f"baseline snapshot path (default: {DEFAULT_BASELINE})",
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.05,
        help="maximum allowed fractional increase per metric (default: 0.05 = 5%%)",
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="overwrite the baseline with the current measurements",
    )
    args = parser.parse_args()

    metrics = run_gas_tests()
    if args.update:
        write_baseline(args.baseline, metrics)
        return 0

    baseline = load_baseline(args.baseline)
    return 0 if check(metrics, baseline, args.threshold) else 1


if __name__ == "__main__":
    raise SystemExit(main())
