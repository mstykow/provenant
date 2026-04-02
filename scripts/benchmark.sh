#!/usr/bin/env bash

set -euo pipefail

REPO_URL=""
REPO_COMMIT=""
TMP_DIR="/tmp/provenant-benchmark"
OUTPUT_DIR="${TMP_DIR}/results"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TARGET_DIR="${TMP_DIR}/target"
SUMMARY_FILE="${OUTPUT_DIR}/summary.tsv"
TARGET_PATH=""
TARGET_LABEL=""
TARGET_SOURCE_LABEL=""

TIME_BIN=""
TIME_ARGS=()
FORWARDED_PROVENANT_ARGS=()

print_header() {
    echo "=========================================="
    echo "Provenant Benchmark Script"
    echo "=========================================="
    echo ""
}

print_usage() {
    cat <<'EOF'
Usage: ./scripts/benchmark.sh [options]

Options:
  --repo-url URL          Clone and benchmark the given repository URL
  --target-path PATH      Benchmark an existing local directory in place
  --repo-commit SHA       Check out this revision after cloning --repo-url
  --help                  Show this help text

Exactly one of --repo-url or --target-path is required.
Benchmark scan flags are required after `--` and are forwarded to Provenant unchanged.
EOF
}

parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --target-path)
                TARGET_PATH="$2"
                shift 2
                ;;
            --repo-url)
                REPO_URL="$2"
                shift 2
                ;;
            --repo-commit)
                REPO_COMMIT="$2"
                shift 2
                ;;
            --)
                shift
                FORWARDED_PROVENANT_ARGS=("$@")
                break
                ;;
            --help)
                print_usage
                exit 0
                ;;
            *)
                echo "Unknown argument: $1" >&2
                print_usage >&2
                exit 1
                ;;
        esac
    done
}

derive_repo_name_from_url() {
    local name="${REPO_URL##*/}"
    name="${name%.git}"
    if [[ -n "$name" ]]; then
        printf '%s\n' "$name"
    else
        printf 'benchmark-target\n'
    fi
}

validate_target_selection() {
    if [[ -n "$REPO_URL" && -n "$TARGET_PATH" ]]; then
        echo "ERROR: specify either --repo-url or --target-path, not both" >&2
        exit 1
    fi

    if [[ -z "$REPO_URL" && -z "$TARGET_PATH" ]]; then
        echo "ERROR: benchmark target required; pass --repo-url or --target-path" >&2
        print_usage >&2
        exit 1
    fi

    if [[ -n "$TARGET_PATH" && -n "$REPO_COMMIT" ]]; then
        echo "ERROR: --repo-commit can only be used with --repo-url" >&2
        exit 1
    fi

    if (( ${#FORWARDED_PROVENANT_ARGS[@]} == 0 )); then
        echo "ERROR: benchmark scan flags required after --" >&2
        print_usage >&2
        exit 1
    fi
}

resolve_target_configuration() {
    if [[ -n "$TARGET_PATH" ]]; then
        TARGET_DIR="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "$TARGET_PATH")"
        if [[ ! -d "$TARGET_DIR" ]]; then
            echo "ERROR: target path not found at ${TARGET_DIR}" >&2
            exit 1
        fi
        TARGET_LABEL="$TARGET_DIR"
        TARGET_SOURCE_LABEL="Target path"
        return
    fi

    TARGET_DIR="${TMP_DIR}/$(derive_repo_name_from_url)"
    TARGET_LABEL="$REPO_URL"
    TARGET_SOURCE_LABEL="Repo URL"
}

determine_target_revision() {
    if git -C "$TARGET_DIR" rev-parse --short HEAD >/dev/null 2>&1; then
        git -C "$TARGET_DIR" rev-parse --short HEAD
        return
    fi

    if [[ -n "$TARGET_PATH" ]]; then
        echo "current local checkout"
        return
    fi

    echo "${REPO_COMMIT:-default branch tip}"
}

detect_time_command() {
    if command -v gtime >/dev/null 2>&1; then
        TIME_BIN="$(command -v gtime)"
        TIME_ARGS=(-v)
        return
    fi

    if /usr/bin/time -v true >/dev/null 2>&1; then
        TIME_BIN="/usr/bin/time"
        TIME_ARGS=(-v)
        return
    fi

    TIME_BIN="/usr/bin/time"
    TIME_ARGS=(-l)
}

portable_now() {
    python3 -c 'import time; print(time.time())'
}

portable_elapsed() {
    local start_time="$1"
    local end_time="$2"
    python3 - "$start_time" "$end_time" <<'PY'
import sys
start = float(sys.argv[1])
end = float(sys.argv[2])
print(f"{end - start:.3f}")
PY
}

parse_json_count() {
    local json_file="$1"
    local key="$2"
    python3 - "$json_file" "$key" <<'PY'
import json
import sys

path, key = sys.argv[1:3]
try:
    with open(path, "r", encoding="utf-8") as handle:
        data = json.load(handle)
    print(len(data.get(key, [])))
except Exception:
    print("N/A")
PY
}

parse_peak_memory_kb() {
    local stdout_file="$1"
    python3 - "$stdout_file" <<'PY'
import re
import sys

path = sys.argv[1]
patterns = [
    (re.compile(r"Maximum resident set size \(kbytes\):\s*(\d+)", re.IGNORECASE), "kb"),
    (re.compile(r"^\s*(\d+)\s+maximum resident set size$", re.IGNORECASE), "bytes"),
]

try:
    with open(path, "r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            for pattern, unit in patterns:
                match = pattern.search(line.strip())
                if match:
                    value = int(match.group(1))
                    if unit == "bytes":
                        value //= 1024
                    print(value)
                    raise SystemExit(0)
except FileNotFoundError:
    pass

print("N/A")
PY
}

extract_phase_seconds() {
    local stdout_file="$1"
    local phase_name="$2"
    python3 - "$stdout_file" "$phase_name" <<'PY'
import re
import sys

path, phase_name = sys.argv[1:3]
pattern = re.compile(rf"^\s*{re.escape(phase_name)}:\s*([0-9]+(?:\.[0-9]+)?)s\s*$")

try:
    with open(path, "r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            match = pattern.search(line)
            if match:
                print(match.group(1))
                raise SystemExit(0)
except FileNotFoundError:
    pass

print("")
PY
}

extract_first_available_phase_seconds() {
    local stdout_file="$1"
    shift

    local phase_name
    for phase_name in "$@"; do
        local value
        value="$(extract_phase_seconds "$stdout_file" "$phase_name")"
        if [[ -n "$value" ]]; then
            printf '%s\n' "$value"
            return
        fi
    done

    printf '\n'
}

extract_summary_line() {
    local stdout_file="$1"
    local label="$2"
    python3 - "$stdout_file" "$label" <<'PY'
import sys

path, label = sys.argv[1:3]
needle = f"{label}:"
last_match = ""

try:
    with open(path, "r", encoding="utf-8", errors="replace") as handle:
        for line in handle:
            stripped = line.strip()
            if needle in stripped:
                last_match = stripped
except FileNotFoundError:
    pass

print(last_match)
PY
}

print_summary_table() {
    python3 - "$SUMMARY_FILE" <<'PY'
import csv
import sys

path = sys.argv[1]
with open(path, newline="", encoding="utf-8") as handle:
    rows = list(csv.DictReader(handle, delimiter="\t"))

if not rows:
    print("No benchmark results recorded.")
    raise SystemExit(0)

headers = [
    ("scenario", "Scenario"),
    ("elapsed_seconds", "Seconds"),
    ("engine_seconds", "Engine s"),
    ("scan_seconds", "Scan s"),
    ("total_seconds", "Total s"),
    ("peak_memory_kb", "Peak KB"),
    ("files_scanned", "Files"),
    ("packages_detected", "Packages"),
    ("incremental_summary", "Incremental summary"),
]

widths = []
for key, label in headers:
    width = len(label)
    for row in rows:
        width = max(width, len(row.get(key, "")))
    widths.append(width)

def format_row(values):
    return " | ".join(value.ljust(width) for value, width in zip(values, widths))

print(format_row([label for _, label in headers]))
print("-+-".join("-" * width for width in widths))
for row in rows:
    print(format_row([row.get(key, "") for key, _ in headers]))

lookup = {row["scenario"]: row for row in rows}

def speedup(metric, baseline, candidate):
    try:
        baseline_value = float(lookup[baseline][metric])
        candidate_value = float(lookup[candidate][metric])
        if candidate_value == 0:
            return None
        return baseline_value / candidate_value
    except Exception:
        return None

comparisons = [
    ("uncached-repeat", "incremental-repeat", "Incremental vs uncached repeat"),
    ("incremental-cold", "incremental-repeat", "Incremental warm vs incremental cold"),
]

print()
for baseline, candidate, label in comparisons:
    wall_result = speedup("elapsed_seconds", baseline, candidate)
    scan_result = speedup("scan_seconds", baseline, candidate)
    if wall_result is not None:
        print(f"{label} (wall): {wall_result:.2f}x speedup")
    if scan_result is not None:
        print(f"{label} (scan): {scan_result:.2f}x speedup")
PY
}

run_case() {
    local scenario="$1"
    local cache_dir="$2"
    local clear_cache_dir="$3"
    shift 3
    local -a extra_args=("$@")

    local scenario_dir="${OUTPUT_DIR}/${scenario}"
    local output_file="${scenario_dir}/scan-output.json"
    local stdout_file="${scenario_dir}/provenant-stdout.txt"
    local -a command

    mkdir -p "$scenario_dir"

    if [[ "$clear_cache_dir" == "true" && -n "$cache_dir" ]]; then
        rm -rf "$cache_dir"
    fi

    echo "------------------------------------------"
    echo "Scenario: ${scenario}"
    if [[ -n "$cache_dir" ]]; then
        echo "  Cache dir: ${cache_dir}"
    else
        echo "  Cache dir: disabled"
    fi
    echo "------------------------------------------"

    local start_time end_time elapsed_seconds files_scanned packages_detected peak_memory_kb
    local engine_seconds scan_seconds total_seconds
    local incremental_summary
    start_time="$(portable_now)"

    command=(
        "$TIME_BIN"
        "${TIME_ARGS[@]}"
        "$PROVENANT_BIN"
        --json
        "$output_file"
    )

    command+=("${FORWARDED_PROVENANT_ARGS[@]}")

    command+=(
        --exclude
        "*.git*"
        --exclude
        "target/*"
    )

    if (( ${#extra_args[@]} > 0 )); then
        command+=("${extra_args[@]}")
    fi

    command+=(.)

    (
        cd "$TARGET_DIR"
        "${command[@]}"
    ) 2>&1 | tee "$stdout_file" | sed 's/^/  /'

    end_time="$(portable_now)"
    elapsed_seconds="$(portable_elapsed "$start_time" "$end_time")"

    files_scanned="$(parse_json_count "$output_file" files)"
    packages_detected="$(parse_json_count "$output_file" packages)"
    peak_memory_kb="$(parse_peak_memory_kb "$stdout_file")"
    engine_seconds="$(extract_first_available_phase_seconds "$stdout_file" setup_scan:licenses finalize:license-engine-creation license_detection_engine_creation)"
    scan_seconds="$(extract_phase_seconds "$stdout_file" scan)"
    total_seconds="$(extract_phase_seconds "$stdout_file" total)"
    incremental_summary="$(extract_summary_line "$stdout_file" Incremental)"

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$scenario" \
        "$elapsed_seconds" \
        "$engine_seconds" \
        "$scan_seconds" \
        "$total_seconds" \
        "$peak_memory_kb" \
        "$files_scanned" \
        "$packages_detected" \
        "$incremental_summary" >>"$SUMMARY_FILE"

    echo ""
    echo "  Wall clock time: ${elapsed_seconds} seconds"
    if [[ -n "$engine_seconds" ]]; then
        echo "  Engine time:     ${engine_seconds} seconds"
    fi
    if [[ -n "$scan_seconds" ]]; then
        echo "  Scan time:       ${scan_seconds} seconds"
    fi
    echo "  Files scanned:   ${files_scanned}"
    echo "  Packages:        ${packages_detected}"
    if [[ "$peak_memory_kb" != "N/A" ]]; then
        echo "  Peak memory:     $((peak_memory_kb / 1024)) MB (${peak_memory_kb} KB)"
    fi
    if [[ -n "$incremental_summary" ]]; then
        echo "  ${incremental_summary}"
    fi
    echo ""
}

parse_arguments "$@"
validate_target_selection
resolve_target_configuration
print_header
detect_time_command

echo "Configuration:"
echo "  ${TARGET_SOURCE_LABEL}: ${TARGET_LABEL}"
echo "  Revision:   $(determine_target_revision)"
echo "  Work dir:   ${TARGET_DIR}"
echo "  Scan args:  ${FORWARDED_PROVENANT_ARGS[*]}"
echo "  Time tool:  ${TIME_BIN} ${TIME_ARGS[*]}"
echo ""

echo "[1/4] Cleaning up previous benchmark directory..."
rm -rf "$TMP_DIR"
mkdir -p "$OUTPUT_DIR"
printf 'scenario\telapsed_seconds\tengine_seconds\tscan_seconds\ttotal_seconds\tpeak_memory_kb\tfiles_scanned\tpackages_detected\tincremental_summary\n' >"$SUMMARY_FILE"

echo "[2/4] Preparing benchmark repository..."
if [[ -n "$TARGET_PATH" ]]; then
    if git -C "$TARGET_DIR" log -1 --oneline >/dev/null 2>&1; then
        git -C "$TARGET_DIR" log -1 --oneline
    else
        echo "  Using local directory without git metadata: ${TARGET_DIR}"
    fi
else
    git clone "$REPO_URL" "$TARGET_DIR" 2>&1 | sed 's/^/  /'
    (
        cd "$TARGET_DIR"
        git checkout "$REPO_COMMIT" 2>&1 | sed 's/^/  /'
        git log -1 --oneline
    )
fi
echo ""

echo "[3/4] Building provenant (release mode)..."
(
    cd "$PROJECT_ROOT"
    cargo build --release 2>&1 | grep -E '(Compiling|Finished|error)' | sed 's/^/  /'
)
echo ""

PROVENANT_BIN="${PROJECT_ROOT}/target/release/provenant"
if [[ ! -x "$PROVENANT_BIN" ]]; then
    echo "ERROR: provenant binary not found at ${PROVENANT_BIN}"
    exit 1
fi

echo "[4/4] Running benchmark matrix..."
echo ""

run_case "uncached-cold" "" "false"
run_case "uncached-repeat" "" "false"
run_case "incremental-cold" "${TMP_DIR}/cache-incremental" "true" \
    --cache-dir "${TMP_DIR}/cache-incremental" --incremental
run_case "incremental-repeat" "${TMP_DIR}/cache-incremental" "false" \
    --cache-dir "${TMP_DIR}/cache-incremental" --incremental

echo "=========================================="
echo "Benchmark Results"
echo "=========================================="
echo ""
print_summary_table

echo ""
echo "Output directories:"
echo "  ${OUTPUT_DIR}/<scenario>/scan-output.json"
echo "  ${OUTPUT_DIR}/<scenario>/provenant-stdout.txt"
echo "  ${SUMMARY_FILE}"
echo ""
echo "To clean up:"
echo "  rm -rf ${TMP_DIR}"
