#!/usr/bin/env python3
"""
Stress Test Log Analyzer

Analyzes JSONL logs from simulation stress tests and generates reports
in multiple formats (JSON, HTML, Markdown).
"""

import argparse
import json
import glob
import sys
from pathlib import Path
from datetime import datetime
from collections import defaultdict, Counter
from typing import Dict, List, Any, Optional, Tuple
import re


class LogEntry:
    """Represents a parsed log entry"""
    def __init__(self, raw_data: Dict[str, Any]):
        self.raw = raw_data
        self.timestamp = raw_data.get('timestamp', '')
        self.level = raw_data.get('level', 'UNKNOWN')
        self.target = raw_data.get('target', '')
        self.message = raw_data.get('message', '')
        self.fields = raw_data.get('fields', {})
        self.span = raw_data.get('span', {})

    def get_field(self, key: str, default=None):
        """Get a field value from fields or span"""
        if key in self.fields:
            return self.fields[key]
        if key in self.span:
            return self.span[key]
        return default


class ScenarioAnalysis:
    """Analysis results for a single scenario"""
    def __init__(self, name: str):
        self.name = name
        self.passed = None  # None = unknown, True/False = determined
        self.start_time = None
        self.end_time = None
        self.duration_ms = None
        self.metrics = {}
        self.assertions = []
        self.errors = []
        self.warnings = []
        self.event_counts = Counter()
        self.trace_ids = set()

    def to_dict(self) -> Dict[str, Any]:
        """Convert to dictionary for JSON serialization"""
        return {
            'name': self.name,
            'passed': self.passed,
            'start_time': self.start_time,
            'end_time': self.end_time,
            'duration_ms': self.duration_ms,
            'metrics': self.metrics,
            'assertions': self.assertions,
            'error_count': len(self.errors),
            'warning_count': len(self.warnings),
            'errors': self.errors[:10],  # Limit to first 10
            'warnings': self.warnings[:10],
            'event_counts': dict(self.event_counts),
            'unique_traces': len(self.trace_ids)
        }


class ThresholdConfig:
    """Threshold configuration for pass/fail criteria"""
    def __init__(self, config: Dict[str, Any]):
        self.thresholds = config.get('thresholds', {})

    def check_metric(self, metric_name: str, value: float) -> Tuple[bool, Optional[str]]:
        """Check if a metric value passes threshold"""
        if metric_name not in self.thresholds:
            return True, None

        threshold = self.thresholds[metric_name]

        if 'min' in threshold and value < threshold['min']:
            return False, f"{metric_name} ({value}) below minimum ({threshold['min']})"
        if 'max' in threshold and value > threshold['max']:
            return False, f"{metric_name} ({value}) above maximum ({threshold['max']})"

        return True, None


class StressAnalyzer:
    """Main analyzer class"""

    def __init__(self, threshold_config: Optional[ThresholdConfig] = None):
        self.scenarios: Dict[str, List[ScenarioAnalysis]] = defaultdict(list)
        self.threshold_config = threshold_config
        self.analysis_timestamp = datetime.utcnow().isoformat() + 'Z'

    def parse_log_file(self, file_path: Path) -> List[LogEntry]:
        """Parse a JSONL log file"""
        entries = []
        line_num = 0

        try:
            with open(file_path, 'r') as f:
                for line in f:
                    line_num += 1
                    line = line.strip()
                    if not line:
                        continue

                    try:
                        data = json.loads(line)
                        entries.append(LogEntry(data))
                    except json.JSONDecodeError as e:
                        print(f"Warning: Failed to parse line {line_num} in {file_path}: {e}",
                              file=sys.stderr)
                        continue

        except Exception as e:
            print(f"Error reading file {file_path}: {e}", file=sys.stderr)

        return entries

    def extract_scenario_name(self, entry: LogEntry) -> Optional[str]:
        """Extract scenario name from log entry"""
        # Try various fields where scenario name might be
        scenario = entry.get_field('scenario')
        if scenario:
            return scenario

        # Try to extract from message
        message = entry.message
        if 'scenario' in message.lower():
            # Pattern: "Running scenario: <name>" or similar
            match = re.search(r'scenario[:\s]+([a-zA-Z0-9_-]+)', message, re.IGNORECASE)
            if match:
                return match.group(1)

        # Check target for scenario runner
        if 'scenario' in entry.target:
            return entry.get_field('name') or 'unknown'

        return None

    def analyze_entries(self, entries: List[LogEntry], file_path: Path):
        """Analyze log entries from a single file"""
        current_scenario = None
        scenario_analysis = None

        for entry in entries:
            # Track scenario context
            scenario_name = self.extract_scenario_name(entry)
            if scenario_name and scenario_name != current_scenario:
                # New scenario started
                if scenario_analysis:
                    self.scenarios[current_scenario].append(scenario_analysis)

                current_scenario = scenario_name
                scenario_analysis = ScenarioAnalysis(scenario_name)
                scenario_analysis.start_time = entry.timestamp

            # Update current scenario analysis
            if scenario_analysis:
                scenario_analysis.end_time = entry.timestamp

                # Track trace IDs
                trace_id = entry.get_field('trace_id')
                if trace_id:
                    scenario_analysis.trace_ids.add(trace_id)

                # Track event types
                event_type = entry.get_field('event_type') or entry.get_field('event')
                if event_type:
                    scenario_analysis.event_counts[event_type] += 1

                # Track errors and warnings
                if entry.level == 'ERROR':
                    scenario_analysis.errors.append({
                        'timestamp': entry.timestamp,
                        'message': entry.message,
                        'target': entry.target
                    })
                elif entry.level == 'WARN':
                    scenario_analysis.warnings.append({
                        'timestamp': entry.timestamp,
                        'message': entry.message,
                        'target': entry.target
                    })

                # Extract metrics from completed scenarios
                if 'completed' in entry.message.lower() or entry.get_field('status') == 'completed':
                    # Extract metrics from fields
                    for key, value in entry.fields.items():
                        if isinstance(value, (int, float)):
                            scenario_analysis.metrics[key] = value

                    # Calculate duration
                    duration = entry.get_field('duration_ms') or entry.get_field('duration')
                    if duration:
                        scenario_analysis.duration_ms = float(duration)

                # Track assertions
                if 'assertion' in entry.message.lower() or entry.get_field('assertion'):
                    assertion_result = {
                        'message': entry.message,
                        'passed': 'passed' in entry.message.lower() or entry.get_field('passed') == True,
                        'expected': entry.get_field('expected'),
                        'actual': entry.get_field('actual')
                    }
                    scenario_analysis.assertions.append(assertion_result)

                    # Update pass/fail status
                    if not assertion_result['passed']:
                        scenario_analysis.passed = False
                    elif scenario_analysis.passed is None:
                        scenario_analysis.passed = True

        # Add final scenario if exists
        if scenario_analysis and current_scenario:
            self.scenarios[current_scenario].append(scenario_analysis)

    def apply_thresholds(self):
        """Apply threshold checks to all scenarios"""
        if not self.threshold_config:
            return

        for scenario_name, analyses in self.scenarios.items():
            for analysis in analyses:
                threshold_failures = []

                for metric_name, metric_value in analysis.metrics.items():
                    passed, message = self.threshold_config.check_metric(metric_name, metric_value)
                    if not passed:
                        threshold_failures.append(message)
                        analysis.passed = False

                if threshold_failures:
                    analysis.assertions.append({
                        'message': 'Threshold checks',
                        'passed': False,
                        'failures': threshold_failures
                    })

    def analyze_files(self, file_pattern: str):
        """Analyze all files matching the pattern"""
        files = glob.glob(file_pattern)

        if not files:
            print(f"Warning: No files found matching pattern: {file_pattern}", file=sys.stderr)
            return

        print(f"Analyzing {len(files)} log file(s)...")

        for file_path in files:
            path = Path(file_path)
            print(f"  Processing: {path.name}")
            entries = self.parse_log_file(path)
            self.analyze_entries(entries, path)

        self.apply_thresholds()

    def generate_summary(self) -> Dict[str, Any]:
        """Generate summary statistics"""
        total_scenarios = sum(len(analyses) for analyses in self.scenarios.values())
        total_passed = sum(
            1 for analyses in self.scenarios.values()
            for a in analyses if a.passed is True
        )
        total_failed = sum(
            1 for analyses in self.scenarios.values()
            for a in analyses if a.passed is False
        )
        total_unknown = total_scenarios - total_passed - total_failed

        return {
            'analysis_timestamp': self.analysis_timestamp,
            'total_scenarios': total_scenarios,
            'unique_scenario_types': len(self.scenarios),
            'passed': total_passed,
            'failed': total_failed,
            'unknown': total_unknown,
            'pass_rate': (total_passed / total_scenarios * 100) if total_scenarios > 0 else 0
        }

    def get_recommendations(self) -> List[str]:
        """Generate recommendations based on failures"""
        recommendations = []

        for scenario_name, analyses in self.scenarios.items():
            failed = [a for a in analyses if a.passed is False]

            if not failed:
                continue

            # Check for common failure patterns
            high_error_rate = any(len(a.errors) > 10 for a in failed)
            low_delivery_rate = any(
                a.metrics.get('delivery_rate', 100) < 90 for a in failed
            )
            high_latency = any(
                a.metrics.get('avg_latency_ms', 0) > 1000 for a in failed
            )

            if high_error_rate:
                recommendations.append(
                    f"{scenario_name}: High error rate detected. Review error logs for root cause."
                )
            if low_delivery_rate:
                recommendations.append(
                    f"{scenario_name}: Low message delivery rate. Check network reliability and retry logic."
                )
            if high_latency:
                recommendations.append(
                    f"{scenario_name}: High latency detected. Consider optimizing critical paths or increasing resources."
                )

        if not recommendations:
            recommendations.append("All tests passed! No recommendations at this time.")

        return recommendations

    def to_json(self) -> Dict[str, Any]:
        """Convert analysis to JSON structure"""
        return {
            'summary': self.generate_summary(),
            'scenarios': {
                name: [analysis.to_dict() for analysis in analyses]
                for name, analyses in self.scenarios.items()
            },
            'recommendations': self.get_recommendations()
        }

    def to_markdown(self) -> str:
        """Generate Markdown report"""
        summary = self.generate_summary()
        lines = [
            "# Stress Test Analysis Report",
            "",
            f"**Generated:** {summary['analysis_timestamp']}",
            "",
            "## Summary",
            "",
            f"- **Total Scenarios:** {summary['total_scenarios']}",
            f"- **Unique Scenario Types:** {summary['unique_scenario_types']}",
            f"- **Passed:** {summary['passed']} ✓",
            f"- **Failed:** {summary['failed']} ✗",
            f"- **Unknown:** {summary['unknown']} ?",
            f"- **Pass Rate:** {summary['pass_rate']:.1f}%",
            "",
            "## Scenario Results",
            ""
        ]

        for scenario_name in sorted(self.scenarios.keys()):
            analyses = self.scenarios[scenario_name]
            lines.append(f"### {scenario_name}")
            lines.append("")

            for i, analysis in enumerate(analyses, 1):
                status = "✓ PASS" if analysis.passed else "✗ FAIL" if analysis.passed is False else "? UNKNOWN"
                lines.append(f"#### Run {i}: {status}")
                lines.append("")

                if analysis.duration_ms:
                    lines.append(f"- **Duration:** {analysis.duration_ms:.0f} ms")
                if analysis.metrics:
                    lines.append(f"- **Metrics:**")
                    for key, value in sorted(analysis.metrics.items()):
                        if isinstance(value, float):
                            lines.append(f"  - {key}: {value:.2f}")
                        else:
                            lines.append(f"  - {key}: {value}")

                if analysis.errors:
                    lines.append(f"- **Errors:** {len(analysis.errors)}")
                if analysis.warnings:
                    lines.append(f"- **Warnings:** {len(analysis.warnings)}")
                if analysis.event_counts:
                    lines.append(f"- **Event Counts:** {dict(analysis.event_counts)}")

                if analysis.assertions:
                    lines.append("- **Assertions:**")
                    for assertion in analysis.assertions:
                        status = "✓" if assertion.get('passed') else "✗"
                        lines.append(f"  - {status} {assertion['message']}")

                lines.append("")

        lines.extend([
            "## Recommendations",
            ""
        ])

        for rec in self.get_recommendations():
            lines.append(f"- {rec}")

        lines.append("")

        return "\n".join(lines)

    def to_html(self) -> str:
        """Generate HTML report"""
        summary = self.generate_summary()

        html = f"""<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>Stress Test Analysis Report</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            max-width: 1200px;
            margin: 0 auto;
            padding: 20px;
            background: #f5f5f5;
        }}
        .container {{
            background: white;
            border-radius: 8px;
            padding: 30px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        h1 {{
            color: #333;
            border-bottom: 3px solid #007bff;
            padding-bottom: 10px;
        }}
        h2 {{
            color: #555;
            margin-top: 30px;
            border-bottom: 2px solid #dee2e6;
            padding-bottom: 8px;
        }}
        h3 {{
            color: #666;
            margin-top: 25px;
        }}
        .summary {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 20px;
            margin: 20px 0;
        }}
        .summary-card {{
            background: #f8f9fa;
            padding: 20px;
            border-radius: 6px;
            border-left: 4px solid #007bff;
        }}
        .summary-card h3 {{
            margin: 0 0 10px 0;
            font-size: 14px;
            color: #666;
            text-transform: uppercase;
        }}
        .summary-card .value {{
            font-size: 32px;
            font-weight: bold;
            color: #333;
        }}
        .pass {{ color: #28a745; }}
        .fail {{ color: #dc3545; }}
        .unknown {{ color: #6c757d; }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 20px 0;
        }}
        th, td {{
            padding: 12px;
            text-align: left;
            border-bottom: 1px solid #dee2e6;
        }}
        th {{
            background: #f8f9fa;
            font-weight: 600;
            color: #495057;
        }}
        tr:hover {{
            background: #f8f9fa;
        }}
        .badge {{
            display: inline-block;
            padding: 4px 8px;
            border-radius: 4px;
            font-size: 12px;
            font-weight: 600;
        }}
        .badge-pass {{
            background: #d4edda;
            color: #155724;
        }}
        .badge-fail {{
            background: #f8d7da;
            color: #721c24;
        }}
        .badge-unknown {{
            background: #e2e3e5;
            color: #383d41;
        }}
        .metrics {{
            background: #f8f9fa;
            padding: 15px;
            border-radius: 4px;
            margin: 10px 0;
        }}
        .metrics dt {{
            font-weight: 600;
            color: #495057;
            margin-top: 8px;
        }}
        .metrics dd {{
            margin-left: 20px;
            color: #666;
        }}
        .recommendations {{
            background: #fff3cd;
            border-left: 4px solid #ffc107;
            padding: 15px;
            margin: 20px 0;
        }}
        .recommendations li {{
            margin: 8px 0;
        }}
        .timestamp {{
            color: #6c757d;
            font-size: 14px;
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Stress Test Analysis Report</h1>
        <p class="timestamp">Generated: {summary['analysis_timestamp']}</p>

        <h2>Summary</h2>
        <div class="summary">
            <div class="summary-card">
                <h3>Total Scenarios</h3>
                <div class="value">{summary['total_scenarios']}</div>
            </div>
            <div class="summary-card">
                <h3>Passed</h3>
                <div class="value pass">{summary['passed']}</div>
            </div>
            <div class="summary-card">
                <h3>Failed</h3>
                <div class="value fail">{summary['failed']}</div>
            </div>
            <div class="summary-card">
                <h3>Pass Rate</h3>
                <div class="value">{summary['pass_rate']:.1f}%</div>
            </div>
        </div>

        <h2>Scenario Results</h2>
"""

        for scenario_name in sorted(self.scenarios.keys()):
            analyses = self.scenarios[scenario_name]
            html += f"        <h3>{scenario_name}</h3>\n"
            html += "        <table>\n"
            html += "            <thead>\n"
            html += "                <tr>\n"
            html += "                    <th>Run</th>\n"
            html += "                    <th>Status</th>\n"
            html += "                    <th>Duration (ms)</th>\n"
            html += "                    <th>Events</th>\n"
            html += "                    <th>Errors</th>\n"
            html += "                    <th>Warnings</th>\n"
            html += "                </tr>\n"
            html += "            </thead>\n"
            html += "            <tbody>\n"

            for i, analysis in enumerate(analyses, 1):
                if analysis.passed is True:
                    status_badge = '<span class="badge badge-pass">PASS</span>'
                elif analysis.passed is False:
                    status_badge = '<span class="badge badge-fail">FAIL</span>'
                else:
                    status_badge = '<span class="badge badge-unknown">UNKNOWN</span>'

                duration = f"{analysis.duration_ms:.0f}" if analysis.duration_ms else "N/A"
                total_events = sum(analysis.event_counts.values())

                html += f"                <tr>\n"
                html += f"                    <td>{i}</td>\n"
                html += f"                    <td>{status_badge}</td>\n"
                html += f"                    <td>{duration}</td>\n"
                html += f"                    <td>{total_events}</td>\n"
                html += f"                    <td>{len(analysis.errors)}</td>\n"
                html += f"                    <td>{len(analysis.warnings)}</td>\n"
                html += f"                </tr>\n"

                if analysis.metrics:
                    html += f"                <tr>\n"
                    html += f"                    <td colspan='6'>\n"
                    html += f"                        <div class='metrics'>\n"
                    html += f"                            <dl>\n"
                    for key, value in sorted(analysis.metrics.items()):
                        if isinstance(value, float):
                            html += f"                                <dt>{key}:</dt><dd>{value:.2f}</dd>\n"
                        else:
                            html += f"                                <dt>{key}:</dt><dd>{value}</dd>\n"
                    html += f"                            </dl>\n"
                    html += f"                        </div>\n"
                    html += f"                    </td>\n"
                    html += f"                </tr>\n"

            html += "            </tbody>\n"
            html += "        </table>\n"

        html += "        <h2>Recommendations</h2>\n"
        html += "        <div class='recommendations'>\n"
        html += "            <ul>\n"

        for rec in self.get_recommendations():
            html += f"                <li>{rec}</li>\n"

        html += """            </ul>
        </div>
    </div>
</body>
</html>"""

        return html


def load_threshold_config(config_path: str) -> Optional[ThresholdConfig]:
    """Load threshold configuration from JSON file"""
    try:
        with open(config_path, 'r') as f:
            config = json.load(f)
            return ThresholdConfig(config)
    except FileNotFoundError:
        print(f"Warning: Threshold config file not found: {config_path}", file=sys.stderr)
        return None
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in threshold config: {e}", file=sys.stderr)
        return None


def main():
    parser = argparse.ArgumentParser(
        description='Analyze JSONL logs from stress tests',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  # Analyze all logs in logs directory, output JSON
  %(prog)s --input "logs/*.jsonl" --output report.json

  # Generate HTML report with thresholds
  %(prog)s --input "logs/*.jsonl" --output report.html --format html --thresholds thresholds.json

  # Generate Markdown report
  %(prog)s --input "logs/*.jsonl" --output report.md --format markdown
        """
    )

    parser.add_argument(
        '--input',
        required=True,
        help='Glob pattern for JSONL log files (e.g., "logs/*.jsonl")'
    )
    parser.add_argument(
        '--output',
        default='reports/stress_report.json',
        help='Output file path (default: reports/stress_report.json)'
    )
    parser.add_argument(
        '--thresholds',
        help='Path to threshold configuration JSON file'
    )
    parser.add_argument(
        '--format',
        choices=['json', 'html', 'markdown'],
        default='json',
        help='Output format (default: json)'
    )

    args = parser.parse_args()

    # Load threshold config if provided
    threshold_config = None
    if args.thresholds:
        threshold_config = load_threshold_config(args.thresholds)

    # Analyze logs
    analyzer = StressAnalyzer(threshold_config)
    analyzer.analyze_files(args.input)

    # Generate output
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    try:
        with open(output_path, 'w') as f:
            if args.format == 'json':
                json.dump(analyzer.to_json(), f, indent=2)
            elif args.format == 'html':
                f.write(analyzer.to_html())
            elif args.format == 'markdown':
                f.write(analyzer.to_markdown())

        print(f"\nAnalysis complete! Report written to: {output_path}")

        # Print summary to console
        summary = analyzer.generate_summary()
        print(f"\nSummary:")
        print(f"  Total scenarios: {summary['total_scenarios']}")
        print(f"  Passed: {summary['passed']}")
        print(f"  Failed: {summary['failed']}")
        print(f"  Pass rate: {summary['pass_rate']:.1f}%")

        # Exit with error code if any tests failed
        sys.exit(1 if summary['failed'] > 0 else 0)

    except Exception as e:
        print(f"Error writing output file: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == '__main__':
    main()
