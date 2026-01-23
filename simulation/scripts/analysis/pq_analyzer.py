#!/usr/bin/env python3
"""
PQ Crypto Stress Test Analyzer

Analyzes JSONL log files from PQ stress test scenarios and generates
performance reports with latency percentiles, throughput metrics, and
pass/fail assessments based on configurable thresholds.

Usage:
    python3 pq_analyzer.py --input logs/*.jsonl --output reports/pq_report.json
    python3 pq_analyzer.py --input logs/pq_baseline.jsonl --thresholds config/thresholds.json
"""

import argparse
import json
import glob
import sys
from pathlib import Path
from dataclasses import dataclass, field, asdict
from typing import List, Dict, Optional, Any
from datetime import datetime
import statistics


@dataclass
class LatencyMetrics:
    """Latency statistics for an operation type."""
    count: int = 0
    total_us: int = 0
    min_us: int = 0
    max_us: int = 0
    p50_us: int = 0
    p95_us: int = 0
    p99_us: int = 0
    avg_us: float = 0.0
    ops_per_sec: float = 0.0


@dataclass
class OperationMetrics:
    """Metrics for a specific PQ operation type."""
    operation: str
    successes: int = 0
    failures: int = 0
    latencies: List[int] = field(default_factory=list)

    def calculate_stats(self) -> LatencyMetrics:
        """Calculate latency statistics from collected samples."""
        if not self.latencies:
            return LatencyMetrics()

        sorted_latencies = sorted(self.latencies)
        count = len(sorted_latencies)

        def percentile(p: int) -> int:
            idx = int(count * p / 100)
            return sorted_latencies[max(0, min(idx, count - 1))]

        avg = statistics.mean(sorted_latencies)
        return LatencyMetrics(
            count=count,
            total_us=sum(sorted_latencies),
            min_us=sorted_latencies[0],
            max_us=sorted_latencies[-1],
            p50_us=percentile(50),
            p95_us=percentile(95),
            p99_us=percentile(99),
            avg_us=avg,
            ops_per_sec=1_000_000 / avg if avg > 0 else 0
        )


@dataclass
class ScenarioResult:
    """Results from a single scenario run."""
    scenario: str
    status: str = "unknown"
    duration_ticks: int = 0
    signature_metrics: Optional[LatencyMetrics] = None
    verification_metrics: Optional[LatencyMetrics] = None
    kem_encap_metrics: Optional[LatencyMetrics] = None
    kem_decap_metrics: Optional[LatencyMetrics] = None
    invite_metrics: Optional[Dict[str, Any]] = None
    threshold_results: Dict[str, Any] = field(default_factory=dict)
    errors: List[str] = field(default_factory=list)


@dataclass
class ThresholdCheck:
    """Result of a threshold check."""
    metric: str
    target: float
    actual: float
    passed: bool


class PQAnalyzer:
    """Analyzes PQ crypto stress test logs."""

    DEFAULT_THRESHOLDS = {
        "signature": {
            "latency_p99_us": 1000,
            "throughput_min": 1000,
            "failure_rate_max": 0.01,
        },
        "verification": {
            "latency_p99_us": 500,
            "throughput_min": 2000,
            "failure_rate_max": 0.01,
        },
        "kem_encap": {
            "latency_p99_us": 200,
            "throughput_min": 5000,
        },
        "kem_decap": {
            "latency_p99_us": 200,
            "throughput_min": 5000,
            "failure_rate_max": 0.01,
        },
        "invite": {
            "success_rate_min": 0.95,
        }
    }

    def __init__(self, thresholds: Optional[Dict] = None):
        self.thresholds = thresholds or self.DEFAULT_THRESHOLDS
        self.operations: Dict[str, OperationMetrics] = {
            "sign": OperationMetrics("sign"),
            "verify": OperationMetrics("verify"),
            "kem_encap": OperationMetrics("kem_encap"),
            "kem_decap": OperationMetrics("kem_decap"),
        }
        self.invites_created = 0
        self.invites_accepted = 0
        self.invites_failed = 0
        self.scenario_name = "unknown"
        self.trace_ids: set = set()

    def parse_log_file(self, filepath: Path) -> None:
        """Parse a JSONL log file and extract PQ metrics."""
        with open(filepath, 'r') as f:
            for line_num, line in enumerate(f, 1):
                line = line.strip()
                if not line:
                    continue
                try:
                    entry = json.loads(line)
                    self._process_log_entry(entry)
                except json.JSONDecodeError as e:
                    print(f"Warning: Invalid JSON at {filepath}:{line_num}: {e}",
                          file=sys.stderr)

    def _process_log_entry(self, entry: Dict) -> None:
        """Process a single log entry."""
        # Parse nested fields if it's a string (from Lua logging)
        fields_raw = entry.get("fields", {})
        if isinstance(fields_raw, str):
            try:
                fields = json.loads(fields_raw)
            except json.JSONDecodeError:
                fields = {}
        else:
            fields = fields_raw

        # Extract scenario name from tags
        if "scenario" in entry:
            self.scenario_name = entry["scenario"]
        elif "scenario" in fields:
            self.scenario_name = fields["scenario"]

        # Track trace IDs
        if "trace_id" in entry:
            self.trace_ids.add(entry["trace_id"])
        elif "trace_id" in fields:
            self.trace_ids.add(fields["trace_id"])

        # Extract message
        msg = entry.get("message", "") or entry.get("msg", "")

        # Parse summary benchmark logs (from Lua scenarios)
        if "operation" in fields:
            op = fields["operation"]
            count = fields.get("count", 0)
            avg_us = fields.get("latency_avg_us", 0)
            p99_us = fields.get("latency_p99_us", 0)
            ops_per_sec = fields.get("ops_per_second", 0)

            if op == "sign" and count > 0:
                # Generate synthetic latency samples based on summary
                for _ in range(min(count, 100)):  # Cap at 100 samples
                    self.operations["sign"].latencies.append(int(avg_us))
                self.operations["sign"].successes = max(self.operations["sign"].successes, count)

            elif op == "verify" and count > 0:
                for _ in range(min(count, 100)):
                    self.operations["verify"].latencies.append(int(avg_us))
                self.operations["verify"].successes = max(self.operations["verify"].successes, count)

            elif op == "encapsulate" and count > 0:
                for _ in range(min(count, 100)):
                    self.operations["kem_encap"].latencies.append(int(avg_us))
                self.operations["kem_encap"].successes = max(self.operations["kem_encap"].successes, count)

            elif op == "decapsulate" and count > 0:
                for _ in range(min(count, 100)):
                    self.operations["kem_decap"].latencies.append(int(avg_us))
                self.operations["kem_decap"].successes = max(self.operations["kem_decap"].successes, count)

        # Parse completion summary logs
        if "total_signatures_created" in fields:
            self.operations["sign"].successes = max(
                self.operations["sign"].successes,
                fields.get("total_signatures_created", 0)
            )
        if "total_signatures_verified" in fields:
            self.operations["verify"].successes = max(
                self.operations["verify"].successes,
                fields.get("total_signatures_verified", 0)
            )
        if "signature_failures" in fields:
            self.operations["verify"].failures = max(
                self.operations["verify"].failures,
                fields.get("signature_failures", 0)
            )
        if "total_kem_encapsulations" in fields:
            self.operations["kem_encap"].successes = max(
                self.operations["kem_encap"].successes,
                fields.get("total_kem_encapsulations", 0)
            )
        if "total_kem_decapsulations" in fields:
            self.operations["kem_decap"].successes = max(
                self.operations["kem_decap"].successes,
                fields.get("total_kem_decapsulations", 0)
            )
        if "kem_failures" in fields:
            self.operations["kem_decap"].failures = max(
                self.operations["kem_decap"].failures,
                fields.get("kem_failures", 0)
            )

        # Invite metrics from summary
        if "invites_created" in fields:
            self.invites_created = max(self.invites_created, fields.get("invites_created", 0))
        if "invites_accepted" in fields:
            self.invites_accepted = max(self.invites_accepted, fields.get("invites_accepted", 0))
        if "invites_failed" in fields:
            self.invites_failed = max(self.invites_failed, fields.get("invites_failed", 0))

        # Add latency samples from summary if available
        if "avg_sign_latency_us" in fields and not self.operations["sign"].latencies:
            avg = fields["avg_sign_latency_us"]
            if avg > 0:
                self.operations["sign"].latencies.append(int(avg))
        if "avg_verify_latency_us" in fields and not self.operations["verify"].latencies:
            avg = fields["avg_verify_latency_us"]
            if avg > 0:
                self.operations["verify"].latencies.append(int(avg))
        if "avg_encap_latency_us" in fields and not self.operations["kem_encap"].latencies:
            avg = fields["avg_encap_latency_us"]
            if avg > 0:
                self.operations["kem_encap"].latencies.append(int(avg))
        if "avg_decap_latency_us" in fields and not self.operations["kem_decap"].latencies:
            avg = fields["avg_decap_latency_us"]
            if avg > 0:
                self.operations["kem_decap"].latencies.append(int(avg))

    def check_thresholds(self, metrics: LatencyMetrics, threshold_key: str) -> List[ThresholdCheck]:
        """Check metrics against thresholds."""
        checks = []
        thresholds = self.thresholds.get(threshold_key, {})

        if "latency_p99_us" in thresholds:
            checks.append(ThresholdCheck(
                metric=f"{threshold_key}_latency_p99_us",
                target=thresholds["latency_p99_us"],
                actual=metrics.p99_us,
                passed=metrics.p99_us <= thresholds["latency_p99_us"]
            ))

        if "throughput_min" in thresholds:
            checks.append(ThresholdCheck(
                metric=f"{threshold_key}_throughput",
                target=thresholds["throughput_min"],
                actual=metrics.ops_per_sec,
                passed=metrics.ops_per_sec >= thresholds["throughput_min"]
            ))

        return checks

    def generate_report(self) -> Dict[str, Any]:
        """Generate a complete analysis report."""
        sign_stats = self.operations["sign"].calculate_stats()
        verify_stats = self.operations["verify"].calculate_stats()
        encap_stats = self.operations["kem_encap"].calculate_stats()
        decap_stats = self.operations["kem_decap"].calculate_stats()

        # Check all thresholds
        all_checks = []
        all_checks.extend(self.check_thresholds(sign_stats, "signature"))
        all_checks.extend(self.check_thresholds(verify_stats, "verification"))
        all_checks.extend(self.check_thresholds(encap_stats, "kem_encap"))
        all_checks.extend(self.check_thresholds(decap_stats, "kem_decap"))

        # Check failure rates
        sign_total = self.operations["sign"].successes + self.operations["sign"].failures
        verify_total = self.operations["verify"].successes + self.operations["verify"].failures
        decap_total = self.operations["kem_decap"].successes + self.operations["kem_decap"].failures

        if verify_total > 0:
            verify_failure_rate = self.operations["verify"].failures / verify_total
            threshold = self.thresholds.get("verification", {}).get("failure_rate_max", 0.01)
            all_checks.append(ThresholdCheck(
                metric="verification_failure_rate",
                target=threshold,
                actual=verify_failure_rate,
                passed=verify_failure_rate <= threshold
            ))

        if decap_total > 0:
            decap_failure_rate = self.operations["kem_decap"].failures / decap_total
            threshold = self.thresholds.get("kem_decap", {}).get("failure_rate_max", 0.01)
            all_checks.append(ThresholdCheck(
                metric="kem_decap_failure_rate",
                target=threshold,
                actual=decap_failure_rate,
                passed=decap_failure_rate <= threshold
            ))

        # Invite success rate
        invite_total = self.invites_accepted + self.invites_failed
        if invite_total > 0:
            invite_success_rate = self.invites_accepted / invite_total
            threshold = self.thresholds.get("invite", {}).get("success_rate_min", 0.95)
            all_checks.append(ThresholdCheck(
                metric="invite_success_rate",
                target=threshold,
                actual=invite_success_rate,
                passed=invite_success_rate >= threshold
            ))

        # Overall status
        all_passed = all(c.passed for c in all_checks)
        failed_checks = [c for c in all_checks if not c.passed]

        report = {
            "test_run_id": list(self.trace_ids)[0] if self.trace_ids else "unknown",
            "timestamp": datetime.utcnow().isoformat() + "Z",
            "scenario": self.scenario_name,
            "status": "pass" if all_passed else "fail",
            "metrics": {
                "signature": {
                    "operations": {
                        "created": self.operations["sign"].successes,
                        "verified": self.operations["verify"].successes,
                        "failures": self.operations["verify"].failures,
                    },
                    "latency": asdict(sign_stats),
                    "verification_latency": asdict(verify_stats),
                },
                "kem": {
                    "operations": {
                        "encapsulations": self.operations["kem_encap"].successes,
                        "decapsulations": self.operations["kem_decap"].successes,
                        "failures": self.operations["kem_decap"].failures,
                    },
                    "encap_latency": asdict(encap_stats),
                    "decap_latency": asdict(decap_stats),
                },
                "invites": {
                    "created": self.invites_created,
                    "accepted": self.invites_accepted,
                    "failed": self.invites_failed,
                    "success_rate": self.invites_accepted / invite_total if invite_total > 0 else 1.0,
                },
            },
            "thresholds": {
                "checks": [asdict(c) for c in all_checks],
                "passed": len([c for c in all_checks if c.passed]),
                "failed": len(failed_checks),
                "total": len(all_checks),
            },
            "summary": {
                "total_pq_operations": (
                    self.operations["sign"].successes +
                    self.operations["verify"].successes +
                    self.operations["kem_encap"].successes +
                    self.operations["kem_decap"].successes
                ),
                "failed_checks": [c.metric for c in failed_checks],
            }
        }

        return report


def main():
    parser = argparse.ArgumentParser(
        description="Analyze PQ crypto stress test logs"
    )
    parser.add_argument(
        "--input", "-i",
        required=True,
        help="Input JSONL file(s), supports glob patterns"
    )
    parser.add_argument(
        "--output", "-o",
        default="pq_report.json",
        help="Output report file (JSON)"
    )
    parser.add_argument(
        "--thresholds", "-t",
        help="Custom thresholds JSON file"
    )
    parser.add_argument(
        "--verbose", "-v",
        action="store_true",
        help="Verbose output"
    )

    args = parser.parse_args()

    # Load custom thresholds if provided
    thresholds = None
    if args.thresholds:
        with open(args.thresholds, 'r') as f:
            thresholds = json.load(f)

    # Initialize analyzer
    analyzer = PQAnalyzer(thresholds)

    # Find and parse input files
    input_files = glob.glob(args.input)
    if not input_files:
        print(f"Error: No files matching '{args.input}'", file=sys.stderr)
        sys.exit(1)

    if args.verbose:
        print(f"Analyzing {len(input_files)} file(s)...")

    for filepath in input_files:
        if args.verbose:
            print(f"  Processing: {filepath}")
        analyzer.parse_log_file(Path(filepath))

    # Generate report
    report = analyzer.generate_report()

    # Write output
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, 'w') as f:
        json.dump(report, f, indent=2)

    # Print summary
    print(f"\n{'=' * 60}")
    print(f"PQ Crypto Stress Test Analysis Report")
    print(f"{'=' * 60}")
    print(f"Scenario: {report['scenario']}")
    print(f"Status: {report['status'].upper()}")
    print(f"Total PQ Operations: {report['summary']['total_pq_operations']:,}")
    print(f"\nThreshold Checks: {report['thresholds']['passed']}/{report['thresholds']['total']} passed")

    if report['thresholds']['failed'] > 0:
        print(f"\nFailed checks:")
        for check in report['thresholds']['checks']:
            if not check['passed']:
                print(f"  - {check['metric']}: {check['actual']:.4f} (target: {check['target']})")

    print(f"\nReport saved to: {args.output}")

    # Exit with appropriate code
    sys.exit(0 if report['status'] == 'pass' else 1)


if __name__ == "__main__":
    main()
