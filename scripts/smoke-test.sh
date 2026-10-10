#!/usr/bin/env bash
#
# Urdia release smoke test.
#
# Builds the workspace from this checkout with a locked dependency set and the
# toolchain the repository pins, installs the binary into a temporary prefix, and
# then exercises the installed copy with the deterministic fake ACP agent.
#
# Two phases have different network needs. The build acquires dependencies into
# an isolated Cargo home of its own, so it can reach crates.io for public crates
# and never inherits the host's Cargo credentials or registry configuration.
# Every check after the build is offline: no AI provider is reachable, because
# each checked process runs under `env -i` with a minimal PATH, an isolated HOME,
# and no credential variable, which also leaves the host agent CLI unresolvable.
# No listener is started; the only bind test asserts that a non-loopback address
# is refused.
#
# Every command is bounded by TERM followed by KILL through a Linux Python
# supervisor. The supervisor remains alive until adopted and detached
# descendants are reaped, and an interrupt is delivered through a request file
# rather than signaling a possibly reaped process group.
#
# The run writes bounded, sanitized evidence atomically: the document is built in
# a temporary file, validated, and renamed into place, and a failure to write,
# validate, or rename it makes the run exit nonzero. It records the schema
# version, run mode, commit identity, toolchain versions, platform, per-check
# name, status and duration, counts, and the overall result. It never records
# prompts, provider responses, arguments, environment contents, credentials, or
# private paths. Evidence and temporary files stay outside the repository.
#
# Exit status is zero only when every check passes, evidence is published, and
# cleanup succeeds.

set -euo pipefail

readonly EVIDENCE_SCHEMA="urdia.smoke-evidence/v1"
readonly MAX_CHECKS=64
readonly MAX_DIAGNOSTIC_CHARS=400
readonly MAX_FAILURE_LINES=64
readonly MAX_EVIDENCE_BYTES=262144

# Credential variables that must never reach a tested process.
readonly FORBIDDEN_ENV_VARS=(
    TYPESAFE_API_KEY
    CREDENTIALS_DIRECTORY
    URDIA_LAYA_API_KEY
    URDIA_LAYA_URL
    URDIA_TYPELLM_API_KEY
    URDIA_TYPELLM_URL
    URDIA_TYPELLM_MODEL
)

usage() {
    cat <<'HELP'
Usage: scripts/smoke-test.sh [options]

  --source-dir PATH     Repository checkout to test (default: this checkout).
  --target-dir PATH     Cargo target directory (default: a temporary one).
  --evidence-dir PATH   Where to write evidence (default: a temporary one).
                        Must be outside the repository working tree.
  --cargo-home PATH     CARGO_HOME to build with (default: an empty temporary
                        one). Must contain no credentials or config file; it is
                        never the host CARGO_HOME.
  --skip-build          Reuse binaries already present in the target directory.
  --keep                Do not delete the temporary root after the run.
  --self-test           Validate the sanitizer and evidence contract only.
                        Requires no toolchain, starts no agent, builds nothing.
  --signal-self-test    Start one long-running check and wait to be signalled.
                        For the external interrupt regression test only; it
                        writes a signal-test-ready marker into the evidence
                        directory naming the top-level pid and the descendant
                        pid files.
  -h, --help            Show this help.

Environment overrides: URDIA_SMOKE_SOURCE_DIR, URDIA_SMOKE_TARGET_DIR,
URDIA_SMOKE_EVIDENCE_DIR, URDIA_SMOKE_CARGO_HOME, URDIA_SMOKE_SKIP_BUILD,
URDIA_SMOKE_KEEP.

The build phase acquires dependencies and may reach crates.io when the Cargo
home is cold. It sends no credentials. Every check after it is offline.
HELP
}

# ---------------------------------------------------------------------------
# Locating the checkout
# ---------------------------------------------------------------------------

resolve_script_path() {
    local source=${BASH_SOURCE[0]}
    while [ -L "$source" ]; do
        local dir
        dir=$(cd -P "$(dirname "$source")" && pwd)
        source=$(readlink "$source")
        [[ $source != /* ]] && source=$dir/$source
    done
    local dir
    dir=$(cd -P "$(dirname "$source")" && pwd)
    printf '%s/%s' "$dir" "$(basename "$source")"
}

SCRIPT_PATH=$(resolve_script_path)
SCRIPT_DIR=$(dirname "$SCRIPT_PATH")
DEFAULT_SOURCE_DIR=$(cd -P "$SCRIPT_DIR/.." && pwd)

# ---------------------------------------------------------------------------
# Options
# ---------------------------------------------------------------------------

SOURCE_DIR=${URDIA_SMOKE_SOURCE_DIR:-$DEFAULT_SOURCE_DIR}
TARGET_DIR=${URDIA_SMOKE_TARGET_DIR:-}
EVIDENCE_DIR=${URDIA_SMOKE_EVIDENCE_DIR:-}
CARGO_HOME_OVERRIDE=${URDIA_SMOKE_CARGO_HOME:-}
SKIP_BUILD=${URDIA_SMOKE_SKIP_BUILD:-0}
KEEP=${URDIA_SMOKE_KEEP:-0}
SELF_TEST=0
SIGNAL_SELF_TEST=0

while [ $# -gt 0 ]; do
    case $1 in
        --source-dir)
            [ $# -ge 2 ] || { echo "smoke: --source-dir needs a path" >&2; exit 2; }
            SOURCE_DIR=$2; shift 2 ;;
        --target-dir)
            [ $# -ge 2 ] || { echo "smoke: --target-dir needs a path" >&2; exit 2; }
            TARGET_DIR=$2; shift 2 ;;
        --evidence-dir)
            [ $# -ge 2 ] || { echo "smoke: --evidence-dir needs a path" >&2; exit 2; }
            EVIDENCE_DIR=$2; shift 2 ;;
        --cargo-home)
            [ $# -ge 2 ] || { echo "smoke: --cargo-home needs a path" >&2; exit 2; }
            CARGO_HOME_OVERRIDE=$2; shift 2 ;;
        --skip-build) SKIP_BUILD=1; shift ;;
        --keep) KEEP=1; shift ;;
        --self-test) SELF_TEST=1; shift ;;
        --signal-self-test) SIGNAL_SELF_TEST=1; shift ;;
        -h | --help) usage; exit 0 ;;
        *)
            echo "smoke: unknown option '$1'" >&2
            usage >&2
            exit 2 ;;
    esac
done

case $SKIP_BUILD in 0 | 1) ;; *) echo "smoke: skip-build must be 0 or 1" >&2; exit 2 ;; esac
case $KEEP in 0 | 1) ;; *) echo "smoke: keep must be 0 or 1" >&2; exit 2 ;; esac
[ -d "$SOURCE_DIR" ] || { echo "smoke: source directory does not exist" >&2; exit 2; }
SOURCE_DIR=$(cd -P "$SOURCE_DIR" && pwd)
# A source directory that is not the workspace would make the build meaningless
# and, worse, would make path redaction substitute a prefix of every other path.
# Reject it explicitly rather than failing in confusing places later.
if [ ! -f "$SOURCE_DIR/Cargo.toml" ] || [ ! -f "$SOURCE_DIR/rust-toolchain.toml" ]; then
    echo "smoke: the source directory is not a Urdia checkout" >&2
    exit 2
fi

# ---------------------------------------------------------------------------
# State
# ---------------------------------------------------------------------------

SMOKE_TMP_PARENT=${TMPDIR:-/tmp}
SMOKE_ROOT=""
SMOKE_HOME=""
SMOKE_PREFIX=""
SMOKE_WORKSPACE=""
SMOKE_CONFIG=""
SMOKE_DATA=""
SMOKE_CARGO_HOME=""
EFFECTIVE_CARGO_HOME=""
LOG_DIR=""
REASON_FILE=""
EVIDENCE_FILE=""
FAILURES_LOG=""
EVIDENCE_WRITTEN=0
EVIDENCE_ERROR=""
RUN_COMPLETED=0
RUN_START_MS=0
RUN_MODE="full"
CHECK_REASON=""
ABORT_REASON=""
LAST_CHECK_STATUS="pass"
TOOLCHAIN_AVAILABLE=0

# Per-run sentinel used as the prompt text. Its appearance in the evidence, in
# the default event database, or in the timeline export is a failure.
SENTINEL=""

# Values captured before any override, so a leak can still be detected.
REAL_HOME=${HOME:-}
RUN_USER=$(id -un 2>/dev/null || echo unknown)
RUN_HOST=$(uname -s 2>/dev/null || echo unknown)
RUN_ARCH=$(uname -m 2>/dev/null || echo unknown)

EVIDENCE_COMMIT="unknown"
EVIDENCE_DIRTY=null
EVIDENCE_RUSTC="unavailable"
EVIDENCE_CARGO="unavailable"

CHECK_NAMES=()
CHECK_STATUSES=()
CHECK_DURATIONS=()

ISOLATED_ENV=()
BUILD_ENV=()
ISOLATED_PATH=""
URDIA_BIN=""
FAKE_AGENT_BIN=""
# The binary CLI checks execute. It stays empty until the install check succeeds
# and the parent shell points it at the installed copy, so no CLI check can
# silently exercise the build tree instead.
URDIA_UNDER_TEST=""

urdia_status=0
urdia_stdout=""
urdia_stderr=""

# A fatal error must never leave evidence claiming success, so it records a
# failed check before exiting. The EXIT trap then writes `result: fail`. The run
# counts as completed because the reason for stopping is known and recorded;
# `incomplete` is reserved for an undiagnosed early exit.
fatal() {
    echo "smoke: $*" >&2
    ABORT_REASON="$*"
    record_failure "aborted" "$*"
    if ((${#CHECK_NAMES[@]} < MAX_CHECKS)); then
        CHECK_NAMES+=("aborted")
        CHECK_STATUSES+=("fail")
        CHECK_DURATIONS+=(0)
    fi
    RUN_COMPLETED=1
    exit 1
}

now_ms() {
    local value
    value=$(date +%s%3N 2>/dev/null || true)
    if [[ $value =~ ^[0-9]{10}[0-9]{3}$ ]]; then
        printf '%s' "$value"
    else
        printf '%s' $((SECONDS * 1000))
    fi
}

drop_last() {
    # usage: drop_last ARRAY_NAME
    local name=$1
    local length
    eval "length=\${#${name}[@]}"
    ((length > 0)) || return 0
    eval "${name}=(\"\${${name}[@]:0:\$((length - 1))}\")"
}

# ---------------------------------------------------------------------------
# Sanitization
# ---------------------------------------------------------------------------

SANITIZE_TEXT=""

# Replaces one private value with a marker. A needle of one character — a path
# that resolved to `/`, say — is a prefix of every other path and would destroy
# the text instead of redacting it, so short needles are skipped.
redact() {
    if [ -n "$1" ] && ((${#1} > 1)); then
        SANITIZE_TEXT=${SANITIZE_TEXT//"$1"/"$2"}
    fi
}

sanitize() {
    SANITIZE_TEXT=$1
    redact "$SMOKE_ROOT" '<smoke-root>'
    redact "$EVIDENCE_DIR" '<evidence>'
    redact "$TARGET_DIR" '<target>'
    redact "$SOURCE_DIR" '<source>'
    redact "$REAL_HOME" '<home>'
    if [ "$RUN_USER" != "unknown" ]; then
        redact "$RUN_USER" '<user>'
    fi
    redact "$SENTINEL" '<sentinel>'
    local text=$SANITIZE_TEXT
    SANITIZE_TEXT=""
    text=${text//$'\n'/ | }
    text=${text//$'\t'/ }
    if ((${#text} > MAX_DIAGNOSTIC_CHARS)); then
        text=${text:0:MAX_DIAGNOSTIC_CHARS}...
    fi
    printf '%s' "$text"
}

json_escape() {
    local text=$1
    text=${text//\\/\\\\}
    text=${text//\"/\\\"}
    # A JSON string may not contain raw control characters. The values escaped
    # here are check names, a commit hash, version strings, and uname output, so
    # flattening whitespace is sufficient and keeps the evidence parseable.
    text=${text//$'\n'/ }
    text=${text//$'\r'/}
    text=${text//$'\t'/ }
    printf '%s' "$text"
}

# ---------------------------------------------------------------------------
# Evidence
# ---------------------------------------------------------------------------

evidence_body() {
    local index=0 entries="" separator=""
    while ((index < ${#CHECK_NAMES[@]})); do
        entries+="$separator"
        entries+="{\"name\":\"$(json_escape "${CHECK_NAMES[$index]}")\""
        entries+=",\"status\":\"${CHECK_STATUSES[$index]}\""
        entries+=",\"duration_ms\":${CHECK_DURATIONS[$index]}}"
        separator=","
        index=$((index + 1))
    done
    printf '%s' "$entries"
}

# Evidence is written atomically and is mandatory. The document is built in a
# temporary file, validated, and only then renamed into place, so a reader never
# sees a truncated file. A failure to write, validate, or rename is reported
# through EVIDENCE_ERROR and makes the run exit nonzero.
emit_evidence() {
    local passed=0 failed=0 index=0 result duration tmp
    while ((index < ${#CHECK_STATUSES[@]})); do
        if [ "${CHECK_STATUSES[$index]}" = "pass" ]; then
            passed=$((passed + 1))
        else
            failed=$((failed + 1))
        fi
        index=$((index + 1))
    done
    if ((failed > 0)); then result="fail"; else result="pass"; fi
    if [ "$EVIDENCE_WRITTEN" = "1" ] && [ "${1:-}" != "force" ]; then
        return 0
    fi
    if [ -z "$EVIDENCE_FILE" ]; then
        EVIDENCE_ERROR="no evidence destination was configured"
        return 1
    fi
    duration=$(( $(now_ms) - RUN_START_MS ))
    tmp="$EVIDENCE_FILE.tmp.$$"
    if ! {
        printf '{\n'
        printf '  "schema_version": "%s",\n' "$EVIDENCE_SCHEMA"
        printf '  "mode": "%s",\n' "$RUN_MODE"
        printf '  "commit": "%s",\n' "$(json_escape "$EVIDENCE_COMMIT")"
        printf '  "commit_dirty": %s,\n' "$EVIDENCE_DIRTY"
        printf '  "toolchain": {"rustc": "%s", "cargo": "%s"},\n' \
            "$(json_escape "$EVIDENCE_RUSTC")" "$(json_escape "$EVIDENCE_CARGO")"
        printf '  "platform": {"os": "%s", "arch": "%s"},\n' \
            "$(json_escape "$RUN_HOST")" "$(json_escape "$RUN_ARCH")"
        printf '  "duration_ms": %d,\n' "$duration"
        printf '  "checks": [%s],\n' "$(evidence_body)"
        printf '  "passed": %d,\n' "$passed"
        printf '  "failed": %d,\n' "$failed"
        printf '  "result": "%s"\n' "$result"
        printf '}\n'
    } >"$tmp" 2>/dev/null; then
        rm -f "$tmp" 2>/dev/null || true
        EVIDENCE_ERROR="could not write the temporary evidence file"
        return 1
    fi
    if ! validate_evidence "$tmp"; then
        rm -f "$tmp" 2>/dev/null || true
        return 1
    fi
    if ! mv -f "$tmp" "$EVIDENCE_FILE" 2>/dev/null; then
        rm -f "$tmp" 2>/dev/null || true
        EVIDENCE_ERROR="could not rename the evidence file into place"
        return 1
    fi
    EVIDENCE_WRITTEN=1
    EVIDENCE_ERROR=""
    return 0
}

# Validates the document this script just produced: shape, bounds, value
# domains, and the sanitization contract. `jq` is used when present but is not
# required, so validation never depends on a tool the host may lack.
validate_evidence() {
    local file=$1 size body
    if [ ! -s "$file" ]; then
        EVIDENCE_ERROR="the evidence file is empty"
        return 1
    fi
    size=$(wc -c <"$file" | tr -d ' ')
    if ((size > MAX_EVIDENCE_BYTES)); then
        EVIDENCE_ERROR="the evidence file exceeds its ${MAX_EVIDENCE_BYTES}-byte bound"
        return 1
    fi
    body=$(cat "$file")
    local trimmed=${body#"${body%%[![:space:]]*}"}
    if [ "${trimmed:0:1}" != "{" ]; then
        EVIDENCE_ERROR="the evidence file does not start with a JSON object"
        return 1
    fi
    # Command substitution strips the trailing newline, so this is the last byte.
    if [ "${body: -1}" != "}" ]; then
        EVIDENCE_ERROR="the evidence file does not end with a JSON object"
        return 1
    fi
    local key
    for key in schema_version mode commit commit_dirty toolchain platform \
        duration_ms checks passed failed result; do
        if [[ $body != *"\"$key\""* ]]; then
            EVIDENCE_ERROR="the evidence file is missing the '$key' field"
            return 1
        fi
    done
    if [[ ! $body =~ \"commit_dirty\":\ (true|false|null), ]]; then
        EVIDENCE_ERROR="commit_dirty is not a JSON boolean or null"
        return 1
    fi
    if [[ ! $body =~ \"result\":\ \"(pass|fail)\" ]]; then
        EVIDENCE_ERROR="result is not 'pass' or 'fail'"
        return 1
    fi
    if [[ ! $body =~ \"passed\":\ [0-9]+, ]]; then
        EVIDENCE_ERROR="passed is not a non-negative integer"
        return 1
    fi
    if [[ ! $body =~ \"failed\":\ [0-9]+, ]]; then
        EVIDENCE_ERROR="failed is not a non-negative integer"
        return 1
    fi
    if [[ ! $body =~ \"duration_ms\":\ [0-9]+, ]]; then
        EVIDENCE_ERROR="duration_ms is not a non-negative integer"
        return 1
    fi
    local names=${body//\"name\":/}
    local recorded=$(( (${#body} - ${#names}) / 7 ))
    local declared_passed declared_failed
    declared_passed=$(sed -n 's/^  "passed": \([0-9][0-9]*\),$/\1/p' "$file")
    declared_failed=$(sed -n 's/^  "failed": \([0-9][0-9]*\),$/\1/p' "$file")
    if [[ ! $declared_passed =~ ^[0-9]+$ ]] || [[ ! $declared_failed =~ ^[0-9]+$ ]]; then
        EVIDENCE_ERROR="the evidence pass and fail counts could not be read"
        return 1
    fi
    if ((recorded != declared_passed + declared_failed)); then
        EVIDENCE_ERROR="the evidence lists $recorded checks but counts $((declared_passed + declared_failed))"
        return 1
    fi
    # The sanitization contract is enforced at write time, not only by a check.
    if [ -n "$SENTINEL" ] && [[ $body == *"$SENTINEL"* ]]; then
        EVIDENCE_ERROR="the evidence file contains sentinel content"
        return 1
    fi
    if [ -n "$REAL_HOME" ] && [[ $body == *"$REAL_HOME"* ]]; then
        EVIDENCE_ERROR="the evidence file contains a home directory path"
        return 1
    fi
    if [ -n "$SOURCE_DIR" ] && [[ $body == *"$SOURCE_DIR"* ]]; then
        EVIDENCE_ERROR="the evidence file contains the checkout path"
        return 1
    fi
    if [ "$RUN_USER" != "unknown" ] && [[ $body == *"$RUN_USER"* ]]; then
        EVIDENCE_ERROR="the evidence file contains the user name"
        return 1
    fi
    if command -v jq >/dev/null 2>&1; then
        if ! jq -e . "$file" >/dev/null 2>&1; then
            EVIDENCE_ERROR="the evidence file is not parseable JSON"
            return 1
        fi
    fi
    return 0
}

record_failure() {
    local name=$1 reason=$2 lines=0
    [ -n "$FAILURES_LOG" ] || return 0
    if [ -f "$FAILURES_LOG" ]; then
        lines=$(wc -l <"$FAILURES_LOG" | tr -d ' ')
    fi
    ((lines < MAX_FAILURE_LINES)) || return 0
    printf '%s: %s\n' "$name" "$(sanitize "$reason")" >>"$FAILURES_LOG"
}

# ---------------------------------------------------------------------------
# Check harness
# ---------------------------------------------------------------------------

fail() {
    # The reason goes to a file because checks run in a subshell, where a global
    # assignment would be lost. `exit` rather than `return` is required: errexit
    # is suppressed for a subshell used as a command condition, so only an
    # explicit exit reliably stops a check at its first failed assertion. Checks
    # always run inside run_check's subshell, so this never ends the script.
    if [ -n "$REASON_FILE" ]; then
        printf '%s' "$*" >"$REASON_FILE"
    else
        printf 'smoke: %s\n' "$*" >&2
    fi
    exit 1
}

expect_zero() {
    [ "$1" = "0" ] || fail "$2: expected exit status 0, got $1"
}

expect_nonzero() {
    [ "$1" != "0" ] || fail "$2: expected a non-zero exit status"
}

expect_contains() {
    [ -n "$2" ] || fail "$3: the expected substring is empty, so the assertion would be vacuous"
    [[ $1 == *"$2"* ]] || fail "$3: output does not contain '$2'"
}

expect_absent() {
    # An empty needle matches everything, which would report a leak that never
    # happened. "Nothing to look for" is treated as satisfied instead.
    [ -n "$2" ] || return 0
    [[ $1 != *"$2"* ]] || fail "$3: output unexpectedly contains '$2'"
}

expect_matches() {
    [[ $1 =~ $2 ]] || fail "$3: value does not match the expected pattern"
}

expect_file() {
    [ -f "$1" ] || fail "$2: expected file is missing"
}

# `unknown` is also the fallback for the commit and toolchain fields, so the user
# name is only a meaningful leak probe when `id` actually resolved it.
expect_no_user_name() {
    [ "$RUN_USER" != "unknown" ] || return 0
    expect_absent "$1" "$RUN_USER" "$2"
}

# Records a check and always returns zero, so `set -e` does not abort the run
# before the remaining checks and the evidence file are produced.
#
# The check body runs in a subshell that is NOT a command condition: errexit is
# suppressed inside `(set -e; ...)` when that subshell is the test of an `if`,
# which would let a check continue past a failed assertion and be recorded on the
# status of its last statement alone. Capturing the status explicitly keeps
# errexit live, so the first failed assertion ends the check.
#
# The subshell runs in the background and the parent waits for it. A foreground
# child defers the parent's signal traps until it exits, so a TERM sent to the
# smoke test during a long check — the release build, say — would be ignored for
# the whole duration of that check. `wait` is interruptible, so the trap runs
# immediately. Cancellation uses the shared request file, then waits for all
# owned jobs to finish cleanup; it does not require process-group signalling.
run_check() {
    local name=$1
    shift
    ((${#CHECK_NAMES[@]} < MAX_CHECKS)) || fatal "too many checks recorded"
    local start_ms duration status=0 had_errexit=0
    case $- in *e*) had_errexit=1 ;; esac
    : >"$REASON_FILE"
    CHECK_REASON=""
    start_ms=$(now_ms)
    set +e
    (set -e; "$@") &
    ACTIVE_CHECK_PID=$!
    # stderr is redirected so bash does not print its own job notice when the
    # check is signalled; the run reports the interruption itself.
    wait "$ACTIVE_CHECK_PID" 2>/dev/null
    status=$?
    ACTIVE_CHECK_PID=""
    if ((had_errexit)); then set -e; fi
    LAST_CHECK_STATUS="pass"
    if ((status != 0)); then
        LAST_CHECK_STATUS="fail"
        if [ -s "$REASON_FILE" ]; then
            CHECK_REASON=$(cat "$REASON_FILE")
        else
            CHECK_REASON="check returned status $status without recording a reason"
        fi
        record_failure "$name" "$CHECK_REASON"
    fi
    duration=$(( $(now_ms) - start_ms ))
    CHECK_NAMES+=("$name")
    CHECK_STATUSES+=("$LAST_CHECK_STATUS")
    CHECK_DURATIONS+=("$duration")
    if [ "$LAST_CHECK_STATUS" = "pass" ]; then
        printf '  pass  %-44s %7d ms\n' "$name" "$duration"
    else
        printf '  FAIL  %-44s %7d ms\n' "$name" "$duration"
        printf '        %s\n' "$(sanitize "$CHECK_REASON")"
    fi
    return 0
}

# A required check aborts the run: later checks cannot be trusted without it.
require_check() {
    local name=$1
    shift
    run_check "$name" "$@"
    if [ "$LAST_CHECK_STATUS" != "pass" ]; then
        echo "smoke: required check '$name' failed; aborting" >&2
        # A diagnosed abort is a completed run whose verdict is fail, so the
        # evidence names the failed check instead of also claiming `incomplete`.
        RUN_COMPLETED=1
        emit_evidence force
        exit 1
    fi
}

count_failures() {
    local index=0 failed=0
    while ((index < ${#CHECK_STATUSES[@]})); do
        [ "${CHECK_STATUSES[$index]}" = "pass" ] || failed=$((failed + 1))
        index=$((index + 1))
    done
    printf '%d' "$failed"
}

# ---------------------------------------------------------------------------
# Isolated execution
# ---------------------------------------------------------------------------

# Bounded execution.
#
# The Python supervisor is mandatory. It starts each command in a new session,
# becomes a Linux child subreaper, and keeps the command leader reserved with
# WNOWAIT while it terminates and reaps every descendant. The supervisor is
# backgrounded and waited on so this script's own traps stay responsive while
# it runs. GNU timeout is intentionally not used for cleanup: it can lose a
# descendant once the command leader exits.
KILL_GRACE_SECONDS=5
PROBE_TIMEOUT_SECONDS=30
URDIA_TIMEOUT_SECONDS=120
BUILD_TIMEOUT_SECONDS=1800
SUPERVISOR_CMD=""
SUPERVISOR_SCRIPT=$SCRIPT_DIR/smoke-supervisor.py
ACTIVE_BOUNDED_PID=""
# Each shell waits on its own check child or supervisor. On cancellation the
# top-level shell joins all of its jobs, including ones not yet assigned here.
ACTIVE_CHECK_PID=""
INTERRUPT_REQUEST_FILE=""
PINNED_TOOLCHAIN=""
BOUNDED_STATUS=0
BOUNDED_OUTPUT=""

require_process_supervisor() {
    if [ "$RUN_HOST" != "Linux" ]; then
        fatal "the smoke supervisor requires Linux"
    fi
    SUPERVISOR_CMD=$(command -v python3 2>/dev/null || true)
    if [ -z "$SUPERVISOR_CMD" ]; then
        fatal "Python 3 is required to supervise bounded commands"
    fi
    [ -f "$SUPERVISOR_SCRIPT" ] ||
        fatal "the smoke process supervisor is missing"
    if ! env -i PATH=/usr/local/bin:/usr/bin:/bin HOME=/tmp LC_ALL=C \
        "$SUPERVISOR_CMD" -I "$SUPERVISOR_SCRIPT" --check >/dev/null 2>&1; then
        fatal "the Linux process supervisor prerequisites are unavailable"
    fi
}

# usage: run_bounded <seconds> <stdout-file> <stderr-file> <command...>
#
# errexit is a shell-wide option, so this restores the caller's state instead of
# re-enabling it unconditionally. A caller that expects the command to fail must
# be able to read the status; a helper that turned errexit back on would abort
# the check before its diagnostics ran.
run_bounded() {
    local seconds=$1 out=$2 err=$3 status=0 had_errexit=0
    shift 3
    case $- in *e*) had_errexit=1 ;; esac
    : >"$out"
    : >"$err"
    if [ -s "$INTERRUPT_REQUEST_FILE" ]; then
        return 143
    fi
    env -i PATH=/usr/local/bin:/usr/bin:/bin HOME="$SMOKE_HOME" LC_ALL=C \
        "$SUPERVISOR_CMD" -I "$SUPERVISOR_SCRIPT" \
        --timeout "$seconds" --kill-after "$KILL_GRACE_SECONDS" \
        --stdout "$out" --stderr "$err" \
        --interrupt-file "$INTERRUPT_REQUEST_FILE" \
        -- "$@" &
    ACTIVE_BOUNDED_PID=$!
    set +e
    # The command owns the stdout/stderr files; supervisor diagnostics remain on
    # the smoke console and never become evidence.
    wait "$ACTIVE_BOUNDED_PID" 2>/dev/null
    status=$?
    if ((had_errexit)); then set -e; fi
    # The supervisor completed descendant cleanup before exiting successfully.
    # Never signal the monitored process here: its PID and process
    # group may be reusable once the supervisor has reaped it.
    ACTIVE_BOUNDED_PID=""
    return "$status"
}

# usage: capture_bounded <seconds> <command...>
# Leaves the exit status in BOUNDED_STATUS and stdout in BOUNDED_OUTPUT.
capture_bounded() {
    local seconds=$1 had_errexit=0
    shift
    case $- in *e*) had_errexit=1 ;; esac
    set +e
    run_bounded "$seconds" "$LOG_DIR/capture-out.txt" "$LOG_DIR/capture-err.txt" "$@"
    BOUNDED_STATUS=$?
    if ((had_errexit)); then set -e; fi
    BOUNDED_OUTPUT=$(cat "$LOG_DIR/capture-out.txt")
}

run_urdia() {
    # Fail closed rather than fall back to the build tree: CLI checks must
    # exercise the binary that was installed into the temporary prefix.
    if [ -z "$URDIA_UNDER_TEST" ]; then
        fail "no installed binary has been selected for CLI checks"
    fi
    local status=0 had_errexit=0
    case $- in *e*) had_errexit=1 ;; esac
    set +e
    run_bounded "$URDIA_TIMEOUT_SECONDS" "$LOG_DIR/stdout.txt" "$LOG_DIR/stderr.txt" \
        "${ISOLATED_ENV[@]}" "$URDIA_UNDER_TEST" "$@"
    status=$?
    if ((had_errexit)); then set -e; fi
    urdia_status=$status
    if [ "$urdia_status" = "124" ] || [ "$urdia_status" = "137" ]; then
        printf 'timed out after %s seconds\n' "$URDIA_TIMEOUT_SECONDS" \
            >>"$LOG_DIR/stderr.txt"
    fi
    urdia_stdout=$(cat "$LOG_DIR/stdout.txt")
    urdia_stderr=$(cat "$LOG_DIR/stderr.txt")
}

# Runs a shell snippet inside the isolated environment and captures its stdout.
isolated_output() {
    capture_bounded "$PROBE_TIMEOUT_SECONDS" "${ISOLATED_ENV[@]}" /bin/sh -c "$1"
    printf '%s' "$BOUNDED_OUTPUT"
}

sentinel_present_in() {
    local path=$1 candidate
    for candidate in "$path" "$path"-wal "$path"-shm; do
        [ -e "$candidate" ] || continue
        if grep -qaF "$SENTINEL" "$candidate" 2>/dev/null; then
            return 0
        fi
    done
    return 1
}

# ---------------------------------------------------------------------------
# Checks
# ---------------------------------------------------------------------------

check_environment_isolation() {
    local name value
    for name in "${FORBIDDEN_ENV_VARS[@]}"; do
        value=$(isolated_output "printf '%s' \"\${$name:-}\"")
        [ -z "$value" ] || fail "the isolated environment still exports $name"
    done
    value=$(isolated_output "printf '%s' \"\$PATH\"")
    [ "$value" = "$ISOLATED_PATH" ] || fail "the isolated PATH was not applied"
    value=$(isolated_output "printf '%s' \"\$HOME\"")
    [ "$value" = "$SMOKE_HOME" ] || fail "the isolated HOME was not applied"
    # The host agent CLI must be invisible, so no hosted inference is reachable.
    value=$(isolated_output "command -v qwen || true")
    [ -z "$value" ] || fail "the host qwen CLI is reachable from the isolated PATH"
    # The build must not inherit the host Cargo credentials or registry
    # configuration either. Only meaningful when a toolchain was configured.
    if [ -n "$EFFECTIVE_CARGO_HOME" ]; then
        capture_bounded "$PROBE_TIMEOUT_SECONDS" "${BUILD_ENV[@]}" /bin/sh -c \
            'printf "%s" "${CARGO_HOME-}"'
        [ "$BOUNDED_OUTPUT" = "$EFFECTIVE_CARGO_HOME" ] ||
            fail "the build did not use the isolated Cargo home"
        local entry
        for entry in credentials credentials.toml config config.toml; do
            if [ -e "$BOUNDED_OUTPUT/$entry" ]; then
                fail "the build Cargo home contains $entry"
            fi
        done
    fi
}

check_prebuilt_binaries() {
    [ -x "$URDIA_BIN" ] || fail "no prebuilt release binary in the target directory"
    [ -x "$FAKE_AGENT_BIN" ] || fail "no prebuilt release fake agent in the target directory"
}

check_toolchain_recorded() {
    expect_matches "$EVIDENCE_RUSTC" '^[0-9]+\.[0-9]+\.[0-9]+' "rustc version"
    expect_matches "$EVIDENCE_CARGO" '^[0-9]+\.[0-9]+\.[0-9]+' "cargo version"
    # The build must run on the toolchain the repository pins rather than on
    # whatever the host defaults to.
    capture_bounded "$PROBE_TIMEOUT_SECONDS" "${BUILD_ENV[@]}" /bin/sh -c \
        'printf "%s" "${RUSTUP_TOOLCHAIN-}"'
    [ "$BOUNDED_OUTPUT" = "$PINNED_TOOLCHAIN" ] ||
        fail "the build selects toolchain '$BOUNDED_OUTPUT', not the pinned '$PINNED_TOOLCHAIN'"
    # And the premise of that fix must still hold: the build does not run inside
    # the checkout, so rustup cannot discover the pin by walking up from its
    # working directory. If it ever did, an unset RUSTUP_TOOLCHAIN would go
    # unnoticed on a host whose default happens to match the pin.
    capture_bounded "$PROBE_TIMEOUT_SECONDS" "${BUILD_ENV[@]}" /bin/sh -c 'pwd'
    [ "$BOUNDED_OUTPUT" != "$SOURCE_DIR" ] ||
        fail "the build ran from the checkout, so toolchain discovery could mask an unset RUSTUP_TOOLCHAIN"
    case $PINNED_TOOLCHAIN in
        [0-9]*.[0-9]*.[0-9]*)
            if [ "$EVIDENCE_RUSTC" != "$PINNED_TOOLCHAIN" ] &&
                [[ $EVIDENCE_RUSTC != "$PINNED_TOOLCHAIN "* ]]; then
                fail "rustc reported '$EVIDENCE_RUSTC' but the repository pins '$PINNED_TOOLCHAIN'"
            fi
            ;;
    esac
}

check_release_build() {
    local status=0 out=$LOG_DIR/build-out.txt err=$LOG_DIR/build-err.txt
    # This is the dependency-acquisition phase and is deliberately separate from
    # the offline runtime checks that follow it: with an isolated, empty
    # CARGO_HOME it may download public crates from crates.io. It cannot send
    # credentials, because the Cargo home it uses is asserted to contain none.
    set +e
    run_bounded "$BUILD_TIMEOUT_SECONDS" "$out" "$err" \
        "${BUILD_ENV[@]}" cargo build --release --locked \
        --target-dir "$TARGET_DIR" --manifest-path "$SOURCE_DIR/Cargo.toml"
    status=$?
    set -e
    if ((status != 0)); then
        # The temporary root is deleted on exit, so surface the compiler output
        # now. This goes to the console only, never into the evidence file.
        echo "smoke: cargo build --release --locked failed with status $status" >&2
        tail -n 40 "$err" >&2 || true
        tail -n 20 "$out" >&2 || true
    fi
    expect_zero "$status" "cargo build --release --locked"
    [ -x "$URDIA_BIN" ] || fail "the release binary was not produced"
    [ -x "$FAKE_AGENT_BIN" ] || fail "the release fake agent was not produced"
}

check_lockfile_unchanged() {
    [ -e "$SOURCE_DIR/.git" ] || return 0
    capture_bounded "$PROBE_TIMEOUT_SECONDS" \
        git -C "$SOURCE_DIR" diff --name-only -- Cargo.lock
    expect_zero "$BOUNDED_STATUS" "git diff on Cargo.lock"
    [ -z "$BOUNDED_OUTPUT" ] || fail "the locked build modified Cargo.lock"
}

check_install_binary() {
    local status=0
    set +e
    run_bounded "$URDIA_TIMEOUT_SECONDS" "$LOG_DIR/install-out.txt" "$LOG_DIR/install.txt" \
        "${ISOLATED_ENV[@]}" /usr/bin/install -Dm755 "$URDIA_BIN" "$SMOKE_PREFIX/bin/urdia"
    status=$?
    set -e
    expect_zero "$status" "install into the temporary prefix"
    [ -x "$SMOKE_PREFIX/bin/urdia" ] || fail "the installed binary is not executable"
}

# Runs in the parent shell on purpose. A check body executes in a subshell, so
# an assignment there would vanish; every later CLI check would keep executing
# the build tree and silently stop testing the installed artifact.
use_installed_binary() {
    if [ ! -x "$SMOKE_PREFIX/bin/urdia" ]; then
        fatal "the installed binary is missing, so later CLI checks cannot be trusted"
    fi
    URDIA_UNDER_TEST=$SMOKE_PREFIX/bin/urdia
}

file_identity() {
    stat -c '%d:%i' "$1" 2>/dev/null || stat -f '%d:%i' "$1" 2>/dev/null || true
}

# Proves the CLI checks exercise the installed copy rather than the build tree.
check_installed_binary_exercised() {
    [ "$URDIA_UNDER_TEST" = "$SMOKE_PREFIX/bin/urdia" ] ||
        fail "CLI checks are not pointed at the installed binary"
    local installed built
    installed=$(file_identity "$URDIA_UNDER_TEST")
    built=$(file_identity "$URDIA_BIN")
    [ -n "$installed" ] || fail "the installed binary could not be identified"
    [ -n "$built" ] || fail "the build output could not be identified"
    [ "$installed" != "$built" ] ||
        fail "the installed path is the build output rather than a separate installed copy"
    run_urdia --version
    expect_zero "$urdia_status" "the installed binary --version"
    expect_matches "$urdia_stdout" '^urdia [0-9]+\.[0-9]+\.[0-9]+' "installed version output"
}

check_version_report() {
    run_urdia --version
    expect_zero "$urdia_status" "urdia --version"
    expect_matches "$urdia_stdout" '^urdia [0-9]+\.[0-9]+\.[0-9]+' "version output"
}

check_doctor() {
    run_urdia doctor
    expect_zero "$urdia_status" "urdia doctor"
    expect_contains "$urdia_stdout" "deny by default" "doctor permission summary"
    expect_contains "$urdia_stdout" "registered providers:" "doctor provider count"
    expect_contains "$urdia_stdout" "provider credentials: not inspected" "doctor credential note"
}

check_providers_listing() {
    run_urdia providers
    expect_zero "$urdia_status" "urdia providers"
    expect_contains "$urdia_stdout" "ID	STATUS	AUTH	INFERENCE" "providers header"
}

write_fake_agent_config() {
    local file=$1 first=1 arg
    shift
    {
        printf '[[provider]]\n'
        printf 'id = "smoke-agent"\n'
        printf 'name = "Urdia Smoke Agent"\n'
        printf 'command = "%s"\n' "$FAKE_AGENT_BIN"
        printf 'args = ['
        for arg in "$@"; do
            if ((first)); then first=0; else printf ', '; fi
            printf '"%s"' "$arg"
        done
        printf ']\n'
    } >"$file"
}

check_external_provider_registration() {
    write_fake_agent_config "$SMOKE_CONFIG/stream.toml" "--stream"
    run_urdia --config "$SMOKE_CONFIG/stream.toml" providers
    expect_zero "$urdia_status" "providers with an external definition"
    expect_contains "$urdia_stdout" "smoke-agent" "external provider row"
    expect_contains "$urdia_stdout" "unknown	unknown	acp-v1,stdio" "conservative metadata"
    run_urdia --config "$SMOKE_CONFIG/stream.toml" doctor
    expect_zero "$urdia_status" "doctor with an external definition"
    expect_contains "$urdia_stdout" "smoke-agent: available" "external availability"
}

check_streamed_run() {
    run_urdia --config "$SMOKE_CONFIG/stream.toml" run \
        --provider smoke-agent \
        --workspace "$SMOKE_WORKSPACE" \
        --prompt "$SENTINEL" \
        --database "$SMOKE_DATA/events.sqlite"
    expect_zero "$urdia_status" "streamed fake-agent run"
    # The fake agent hardcodes its own label; the registry ID is not part of it.
    expect_contains "$urdia_stdout" "fake-agent: $SENTINEL" "streamed response"
    expect_file "$SMOKE_DATA/events.sqlite" "the event database"
}

# Proves the retention detector can find retained text, so the negative
# assertions below cannot pass vacuously.
check_content_retention_positive_control() {
    run_urdia --config "$SMOKE_CONFIG/stream.toml" run \
        --provider smoke-agent \
        --workspace "$SMOKE_WORKSPACE" \
        --prompt "$SENTINEL" \
        --database "$SMOKE_DATA/persist.sqlite" \
        --persist-content
    expect_zero "$urdia_status" "opt-in content retention run"
    if ! sentinel_present_in "$SMOKE_DATA/persist.sqlite"; then
        fail "the detector found no sentinel in a database written with --persist-content"
    fi
}

check_default_run_retains_no_content() {
    expect_file "$SMOKE_DATA/events.sqlite" "the default event database"
    if sentinel_present_in "$SMOKE_DATA/events.sqlite"; then
        fail "the default database retained prompt or response content"
    fi
}

check_deny_by_default_permission() {
    write_fake_agent_config "$SMOKE_CONFIG/permission.toml" "--request-permission"
    run_urdia --config "$SMOKE_CONFIG/permission.toml" run \
        --provider smoke-agent \
        --workspace "$SMOKE_WORKSPACE" \
        --prompt "$SENTINEL" \
        --database "$SMOKE_DATA/permission.sqlite"
    expect_zero "$urdia_status" "deny-by-default run"
    expect_contains "$urdia_stdout" "permission rejected" "the rejection reached the agent"
    expect_absent "$urdia_stdout" "Urdia permission UI" "no permission UI was started"
    expect_file "$SMOKE_DATA/permission.sqlite" "the receipt database"
    if sentinel_present_in "$SMOKE_DATA/permission.sqlite"; then
        fail "the receipt database retained prompt or response content"
    fi
}

check_timeline_export() {
    local html=$SMOKE_DATA/timeline.html
    run_urdia timeline --database "$SMOKE_DATA/events.sqlite" --output "$html"
    expect_zero "$urdia_status" "urdia timeline"
    expect_file "$html" "the timeline export"
    local mode
    mode=$(stat -c '%a' "$html" 2>/dev/null || stat -f '%Lp' "$html" 2>/dev/null || true)
    [ -n "$mode" ] || fail "the timeline export mode could not be determined"
    [ "$mode" = "600" ] || fail "the timeline export has mode $mode instead of 600"
    if grep -qaF "$SENTINEL" "$html" 2>/dev/null; then
        fail "the timeline export retained sentinel content"
    fi
    if grep -qaF "$SMOKE_WORKSPACE" "$html" 2>/dev/null; then
        fail "the timeline export leaked the workspace path"
    fi
    if grep -qaF "$(basename "$SMOKE_WORKSPACE")" "$html" 2>/dev/null; then
        fail "the timeline export leaked the workspace name"
    fi
}

check_deterministic_route() {
    run_urdia --config "$SMOKE_CONFIG/stream.toml" route \
        --objective "Review this workspace" \
        --router deterministic \
        --candidate smoke-agent
    expect_zero "$urdia_status" "deterministic route"
    expect_contains "$urdia_stdout" "router: deterministic" "the router label"
    expect_contains "$urdia_stdout" "recommended provider: smoke-agent" "the recommendation"
    expect_contains "$urdia_stdout" \
        "advisory only: no agent started and no permission granted" "the advisory notice"
}

check_deterministic_eval_routing() {
    local fixture=$SOURCE_DIR/evaluations/routing/v2.json
    expect_file "$fixture" "the v2 routing fixture"
    run_urdia eval-routing --fixture "$fixture" --router deterministic \
        --split held-out --repeats 1
    expect_zero "$urdia_status" "deterministic eval-routing"
    expect_contains "$urdia_stdout" '"schema_version": "urdia.routing-evaluation/' \
        "the report schema"
    expect_contains "$urdia_stdout" '"router": "deterministic"' "the report router"
    expect_contains "$urdia_stdout" '"corpus_id": "urdia-routing-v2"' "the report corpus"
}

# Proves a non-loopback listener is refused before any socket is created.
check_non_loopback_bind_rejected() {
    run_urdia ui --database "$SMOKE_DATA/events.sqlite" --bind 0.0.0.0:0
    expect_nonzero "$urdia_status" "a non-loopback ui bind"
    expect_contains "$urdia_stderr" "must be loopback" "the loopback rejection message"
}

# Proves no credential was inherited: each advisory provider must fail on
# configuration instead of reaching a network endpoint.
check_decision_providers_unconfigured() {
    local router
    for router in jev laya typellm; do
        run_urdia --config "$SMOKE_CONFIG/stream.toml" route \
            --objective "Review this workspace" --router "$router" --candidate smoke-agent
        expect_nonzero "$urdia_status" "the $router route without credentials"
        expect_contains "$urdia_stderr" "configuration is invalid" "the $router redacted error"
    done
}

check_uninstall() {
    rm -f "$SMOKE_PREFIX/bin/urdia"
    [ ! -e "$SMOKE_PREFIX/bin/urdia" ] || fail "the installed binary survived removal"
    local resolved
    resolved=$(isolated_output "command -v urdia || true")
    [ -z "$resolved" ] || fail "urdia is still resolvable after uninstall"
    rm -rf "$SMOKE_DATA"
    [ ! -e "$SMOKE_DATA" ] || fail "workspace event data survived removal"
}

# The evidence recorded so far must not carry the sentinel, a host path, a user
# name, or the checkout location.
check_evidence_sanitized() {
    local body
    body=$(evidence_body)
    expect_absent "$body" "$SENTINEL" "the evidence checks"
    expect_absent "$body" "$REAL_HOME" "the evidence checks"
    expect_absent "$body" "$SOURCE_DIR" "the evidence checks"
    expect_no_user_name "$body" "the evidence checks"
    if [ -f "$FAILURES_LOG" ]; then
        local logged
        logged=$(cat "$FAILURES_LOG")
        expect_absent "$logged" "$SENTINEL" "the failure log"
        expect_absent "$logged" "$REAL_HOME" "the failure log"
        expect_absent "$logged" "$SOURCE_DIR" "the failure log"
    fi
}

# ---------------------------------------------------------------------------
# Setup
# ---------------------------------------------------------------------------

reject_paths_inside_repository() {
    local label=$1 path=$2 resolved
    resolved=$(cd -P "$(dirname "$path")" 2>/dev/null && pwd)/$(basename "$path")
    case $resolved in
        "$SOURCE_DIR" | "$SOURCE_DIR"/*)
            fatal "$label must stay outside the repository working tree" ;;
    esac
}

create_isolated_roots() {
    SMOKE_ROOT=$(mktemp -d "$SMOKE_TMP_PARENT/urdia-smoke.XXXXXXXX")
    reject_paths_inside_repository "the temporary root" "$SMOKE_ROOT"
    SMOKE_HOME=$SMOKE_ROOT/home
    SMOKE_PREFIX=$SMOKE_ROOT/prefix
    SMOKE_WORKSPACE=$SMOKE_ROOT/smoke-ws-$$
    SMOKE_CONFIG=$SMOKE_ROOT/config
    SMOKE_DATA=$SMOKE_ROOT/data
    SMOKE_CARGO_HOME=$SMOKE_ROOT/cargo-home
    LOG_DIR=$SMOKE_ROOT/logs
    REASON_FILE=$LOG_DIR/reason.txt
    INTERRUPT_REQUEST_FILE=$LOG_DIR/interrupt.request
    mkdir -p "$SMOKE_HOME" "$SMOKE_PREFIX/bin" "$SMOKE_WORKSPACE" \
        "$SMOKE_CONFIG" "$SMOKE_DATA" "$SMOKE_CARGO_HOME" "$LOG_DIR" \
        "$SMOKE_ROOT/tmp"
    chmod 700 "$SMOKE_ROOT"
    : >"$REASON_FILE"
    rm -f -- "$INTERRUPT_REQUEST_FILE"
    URDIA_UNDER_TEST=""

    if [ -z "$EVIDENCE_DIR" ]; then
        EVIDENCE_DIR=$(mktemp -d "$SMOKE_TMP_PARENT/urdia-smoke-evidence.XXXXXXXX")
    else
        mkdir -p "$EVIDENCE_DIR"
        EVIDENCE_DIR=$(cd -P "$EVIDENCE_DIR" && pwd)
    fi
    reject_paths_inside_repository "the evidence directory" "$EVIDENCE_DIR"
    EVIDENCE_FILE=$EVIDENCE_DIR/smoke-evidence.json
    FAILURES_LOG=$EVIDENCE_DIR/failures.log
    : >"$FAILURES_LOG"

    if [ -z "$TARGET_DIR" ]; then
        TARGET_DIR=$SMOKE_ROOT/target
    else
        mkdir -p "$TARGET_DIR"
        TARGET_DIR=$(cd -P "$TARGET_DIR" && pwd)
    fi
    # The target directory is deliberately NOT passed through
    # reject_paths_inside_repository: CI points it at the workspace's own
    # `target/` so the Rust cache action can reuse it. That is safe because
    # `.gitignore` excludes `/target/`, the build is `--locked`, and
    # `lockfile-unchanged` asserts Cargo.lock survived it.
    if [ -n "$CARGO_HOME_OVERRIDE" ]; then
        mkdir -p "$CARGO_HOME_OVERRIDE"
        CARGO_HOME_OVERRIDE=$(cd -P "$CARGO_HOME_OVERRIDE" && pwd)
        reject_paths_inside_repository "the Cargo home" "$CARGO_HOME_OVERRIDE"
    fi

    SENTINEL="urdia-smoke-sentinel-$$-$(now_ms)"
    ISOLATED_PATH="$SMOKE_PREFIX/bin:/usr/bin:/bin"
    ISOLATED_ENV=(env -i
        "PATH=$ISOLATED_PATH"
        "HOME=$SMOKE_HOME"
        "TMPDIR=$SMOKE_ROOT/tmp"
        "LC_ALL=C")
    URDIA_BIN=$TARGET_DIR/release/urdia
    FAKE_AGENT_BIN=$TARGET_DIR/release/urdia-fake-agent
}

record_identity() {
    capture_bounded "$PROBE_TIMEOUT_SECONDS" git -C "$SOURCE_DIR" rev-parse --git-dir
    ((BOUNDED_STATUS == 0)) || return 0
    capture_bounded "$PROBE_TIMEOUT_SECONDS" git -C "$SOURCE_DIR" rev-parse HEAD
    if ((BOUNDED_STATUS == 0)) && [ -n "$BOUNDED_OUTPUT" ]; then
        EVIDENCE_COMMIT=$BOUNDED_OUTPUT
    fi
    capture_bounded "$PROBE_TIMEOUT_SECONDS" git -C "$SOURCE_DIR" status --porcelain
    # Only a successful empty listing proves a clean tree. A git failure must be
    # reported as unknown rather than as clean, and `null` keeps the JSON valid.
    if ((BOUNDED_STATUS != 0)); then
        EVIDENCE_DIRTY=null
    elif [ -n "$BOUNDED_OUTPUT" ]; then
        EVIDENCE_DIRTY=true
    else
        EVIDENCE_DIRTY=false
    fi
}

record_toolchain() {
    capture_bounded "$PROBE_TIMEOUT_SECONDS" "${BUILD_ENV[@]}" rustc --version
    if ((BOUNDED_STATUS == 0)) && [ -n "$BOUNDED_OUTPUT" ]; then
        EVIDENCE_RUSTC=${BOUNDED_OUTPUT#rustc }
        TOOLCHAIN_AVAILABLE=1
    fi
    capture_bounded "$PROBE_TIMEOUT_SECONDS" "${BUILD_ENV[@]}" cargo --version
    if ((BOUNDED_STATUS == 0)) && [ -n "$BOUNDED_OUTPUT" ]; then
        EVIDENCE_CARGO=${BOUNDED_OUTPUT#cargo }
        EVIDENCE_CARGO=${EVIDENCE_CARGO%% *}
    else
        TOOLCHAIN_AVAILABLE=0
    fi
    # Version strings are interpolated into JSON, so they must stay single-line
    # and free of control characters.
    EVIDENCE_RUSTC=$(first_line "$EVIDENCE_RUSTC")
    EVIDENCE_CARGO=$(first_line "$EVIDENCE_CARGO")
}

first_line() {
    local text=${1%%$'\n'*}
    text=${text//$'\r'/}
    text=${text//$'\t'/ }
    printf '%s' "$text"
}

configure_build_env() {
    local cargo_path rustc_path build_path
    read_pinned_toolchain
    cargo_path=$(command -v cargo 2>/dev/null || true)
    rustc_path=$(command -v rustc 2>/dev/null || true)
    if [ -z "$cargo_path" ] || [ -z "$rustc_path" ]; then
        BUILD_ENV=("${ISOLATED_ENV[@]}" "RUSTUP_TOOLCHAIN=$PINNED_TOOLCHAIN")
        return 0
    fi
    build_path=$(cd -P "$(dirname "$cargo_path")" && pwd):$(cd -P "$(dirname "$rustc_path")" && pwd)
    EFFECTIVE_CARGO_HOME=${CARGO_HOME_OVERRIDE:-$SMOKE_CARGO_HOME}
    assert_clean_cargo_home "$EFFECTIVE_CARGO_HOME"
    # The host CARGO_HOME is deliberately not inherited: it can hold a crates.io
    # token and private registry configuration. RUSTUP_HOME is inherited because
    # it is a toolchain store rather than a credential store, and isolating it
    # would force a toolchain download. `-C` starts cargo inside the temporary
    # root so it cannot pick up a project-level `.cargo/config.toml` from the
    # directory the script was invoked in.
    #
    # That same `-C` is why RUSTUP_TOOLCHAIN is set explicitly. rustup discovers
    # `rust-toolchain.toml` by walking up from the working directory, and the
    # build does not run inside the checkout, so discovery finds nothing and the
    # shims would silently fall back to the host default toolchain. Selecting the
    # repository pin by name keeps the evidence tied to the toolchain the project
    # actually requires.
    local rustup_home=${RUSTUP_HOME:-$REAL_HOME/.rustup}
    BUILD_ENV=(env -i -C "$SMOKE_ROOT"
        "PATH=$build_path:/usr/local/bin:/usr/bin:/bin"
        "HOME=$SMOKE_HOME"
        "TMPDIR=$SMOKE_ROOT/tmp"
        "LC_ALL=C"
        "CARGO_TARGET_DIR=$TARGET_DIR"
        "CARGO_HOME=$EFFECTIVE_CARGO_HOME"
        "RUSTUP_TOOLCHAIN=$PINNED_TOOLCHAIN")
    if [ -d "$rustup_home" ]; then
        BUILD_ENV+=("RUSTUP_HOME=$rustup_home")
    fi
}

# Reads the channel the repository pins. Parsed without an external process, and
# only the first `channel` key is honoured, which is all `rust-toolchain.toml`
# defines.
read_pinned_toolchain() {
    local file=$SOURCE_DIR/rust-toolchain.toml line value
    if [ ! -f "$file" ]; then
        fatal "the checkout does not pin a toolchain in rust-toolchain.toml"
    fi
    while IFS= read -r line || [ -n "$line" ]; do
        line=${line%%#*}
        case $line in
            *channel*=*)
                value=${line#*=}
                value=${value//[[:space:]]/}
                value=${value#\"}
                value=${value%\"}
                if [ -n "$value" ]; then
                    PINNED_TOOLCHAIN=$value
                    break
                fi
                ;;
        esac
    done <"$file"
    if [ -z "$PINNED_TOOLCHAIN" ]; then
        fatal "could not read a toolchain channel from rust-toolchain.toml"
    fi
}

# Cargo reads credentials and private registry configuration from its home, so
# the home this script builds with must provably contain neither.
assert_clean_cargo_home() {
    local home=$1 entry
    if [ ! -d "$home" ]; then
        fatal "the Cargo home to build with does not exist"
    fi
    for entry in credentials credentials.toml config config.toml; do
        if [ -e "$home/$entry" ]; then
            fatal "the Cargo home contains $entry; refusing to build with inherited Cargo credentials or configuration"
        fi
    done
}

# Stops the active check and everything it started. The request file is polled
# by the supervisor, including before it starts a command, so this handler
# never signals a PID or process group that Bash may already have reaped.
stop_descendants() {
    if [ -n "$INTERRUPT_REQUEST_FILE" ]; then
        local temporary="$INTERRUPT_REQUEST_FILE.tmp.$$"
        printf '%s\n' "interrupt" >"$temporary" 2>/dev/null || true
        mv -f -- "$temporary" "$INTERRUPT_REQUEST_FILE" 2>/dev/null || true
    fi
    # Every background job belongs to this harness: direct bounded supervisors
    # and check children. Waiting without an operand closes the fork-to-PID-file
    # window and lets each supervisor finish descendant reaping.
    wait 2>/dev/null || true
    ACTIVE_CHECK_PID=""
    ACTIVE_BOUNDED_PID=""
}

# An interrupt must not be reported as a clean run: `$?` inside an EXIT trap is
# the previous command's status, which can be zero.
on_signal() {
    local name=$1 code=$2
    trap '' INT TERM
    echo "smoke: interrupted by $name" >&2
    stop_descendants
    record_failure "interrupted" "the run was interrupted by $name"
    CHECK_NAMES+=("interrupted")
    CHECK_STATUSES+=("fail")
    CHECK_DURATIONS+=(0)
    RUN_COMPLETED=1
    exit "$code"
}

install_traps() {
    trap cleanup EXIT
    trap 'on_signal INT 130' INT
    trap 'on_signal TERM 143' TERM
}

cleanup() {
    local status=$?
    trap - EXIT
    trap '' INT TERM
    # errexit is live inside a trap, so every step is guarded: failing to write
    # evidence must never prevent the temporary root from being removed.
    set +e
    # Stop any command still running under a bound before deleting the
    # directories it uses, so nothing can recreate them afterwards.
    stop_descendants
    # A run that ended before it finished must never be reported as a pass,
    # whatever the exit status of the command that happened to fail.
    if [ "$RUN_COMPLETED" != "1" ] && [ -n "$EVIDENCE_FILE" ]; then
        record_failure "incomplete" "the run ended before it completed"
        CHECK_NAMES+=("incomplete")
        CHECK_STATUSES+=("fail")
        CHECK_DURATIONS+=(0)
        status=1
    fi
    # Evidence is mandatory. A run that cannot publish it did not succeed,
    # whatever its own checks reported.
    if ! emit_evidence force; then
        echo "smoke: ${EVIDENCE_ERROR:-could not write the evidence file}" >&2
        status=1
    fi
    if [ -n "$SMOKE_ROOT" ] && [ "$KEEP" != "1" ]; then
        rm -rf -- "$SMOKE_ROOT"
        if [ -e "$SMOKE_ROOT" ]; then
            echo "smoke: cleanup could not remove the temporary root" >&2
            record_failure "cleanup" "the temporary root could not be removed"
            CHECK_NAMES+=("cleanup")
            CHECK_STATUSES+=("fail")
            CHECK_DURATIONS+=(0)
            emit_evidence force >/dev/null 2>&1 || true
            status=1
        fi
    fi
    exit "$status"
}

# ---------------------------------------------------------------------------
# Self-test mode
# ---------------------------------------------------------------------------

self_test_sanitizer() {
    local sanitized
    sanitized=$(sanitize "$REAL_HOME/$RUN_USER/private $SOURCE_DIR/crate $SENTINEL")
    expect_absent "$sanitized" "$REAL_HOME" "the sanitized text"
    expect_absent "$sanitized" "$SOURCE_DIR" "the sanitized text"
    expect_absent "$sanitized" "$SENTINEL" "the sanitized text"
    expect_contains "$sanitized" "<home>" "the sanitized text"
    expect_contains "$sanitized" "<source>" "the sanitized text"
    expect_contains "$sanitized" "<sentinel>" "the sanitized text"
}

self_test_bounds() {
    local long="" index=0 sanitized escaped
    while ((index < MAX_DIAGNOSTIC_CHARS + 50)); do
        long+="x"
        index=$((index + 1))
    done
    sanitized=$(sanitize "$long")
    ((${#sanitized} <= MAX_DIAGNOSTIC_CHARS + 3)) || fail "diagnostics are not bounded"
    [[ $(sanitize $'one\ntwo\tthree') != *$'\n'* ]] || fail "diagnostics are not single-line"
    escaped=$(json_escape 'quote " backslash \ end')
    [ "$escaped" = 'quote \" backslash \\ end' ] || fail "JSON escaping is wrong"
}

self_test_failure_log_sanitized() {
    record_failure "self-test-plant" \
        "planted $SENTINEL under $REAL_HOME in $SOURCE_DIR"
    local logged
    logged=$(cat "$FAILURES_LOG")
    expect_contains "$logged" "self-test-plant" "the failure log"
    expect_contains "$logged" "<sentinel>" "the failure log"
    expect_absent "$logged" "$SENTINEL" "the failure log"
    expect_absent "$logged" "$REAL_HOME" "the failure log"
    expect_absent "$logged" "$SOURCE_DIR" "the failure log"
    : >"$FAILURES_LOG"
}

synthetic_failing_check() {
    expect_contains "abc" "zzz" "first assertion"
    expect_contains "abc" "yyy" "second assertion"
    printf 'side-effect-ran\n' >"$SMOKE_ROOT/side-effect.txt"
}

# A failing check must stop at its first failed assertion, so the recorded
# reason is the real cause and no later statement can run a side effect.
self_test_first_failure_wins() {
    local before=${#CHECK_NAMES[@]}
    run_check "self-test-synthetic-failing-check" synthetic_failing_check >/dev/null
    [ "${CHECK_STATUSES[$before]}" = "fail" ] ||
        fail "a failing check was recorded as passing"
    local logged
    logged=$(cat "$FAILURES_LOG")
    expect_contains "$logged" "first assertion" "the failure log"
    expect_absent "$logged" "second assertion" "the failure log"
    [ ! -e "$SMOKE_ROOT/side-effect.txt" ] ||
        fail "statements after a failed assertion still ran"
    # Drop the synthetic record so a healthy self-test still reports pass.
    drop_last CHECK_NAMES
    drop_last CHECK_STATUSES
    drop_last CHECK_DURATIONS
    : >"$FAILURES_LOG"
}

# Regression guard for the installed-binary rule: a CLI check must run the copy
# installed into the temporary prefix, and must refuse to run at all before one
# has been selected. Both halves matter, because a check body runs in a subshell
# where an assignment cannot reach the parent.
self_test_installed_binary_is_selected() {
    printf '#!/bin/sh\necho build-tree-stub\n' >"$SMOKE_ROOT/build-stub"
    printf '#!/bin/sh\necho installed-stub\n' >"$SMOKE_PREFIX/bin/urdia"
    chmod 755 "$SMOKE_ROOT/build-stub" "$SMOKE_PREFIX/bin/urdia"
    URDIA_BIN=$SMOKE_ROOT/build-stub
    URDIA_UNDER_TEST=""

    local status=0
    set +e
    (run_urdia --version) >/dev/null 2>&1
    status=$?
    set -e
    [ "$status" != "0" ] ||
        fail "a CLI check ran before an installed binary had been selected"

    use_installed_binary
    [ "$URDIA_UNDER_TEST" = "$SMOKE_PREFIX/bin/urdia" ] ||
        fail "the installed binary was not selected for CLI checks"
    run_urdia --version
    expect_zero "$urdia_status" "the installed stub"
    [ "$urdia_stdout" = "installed-stub" ] ||
        fail "the build tree binary was executed instead of the installed copy"
}

# A command that exits successfully while leaving a child behind must still
# return status zero only after the supervisor has cleaned that child.
self_test_bounded_command_cleans_after_successful_parent_exit() {
    local stub=$SMOKE_ROOT/early-exit-stub.sh
    local childfile=$SMOKE_ROOT/early-exit-child.pid
    cat >"$stub" <<'STUB'
#!/bin/sh
sleep 300 &
printf '%s\n' "$!" >"$1"
exit 0
STUB
    chmod 755 "$stub"

    local status=0
    set +e
    run_bounded 10 "$LOG_DIR/early-exit-out.txt" "$LOG_DIR/early-exit-err.txt" \
        /bin/sh "$stub" "$childfile"
    status=$?
    set -e
    [ "$status" = "0" ] ||
        fail "a successful parent with a background child returned status $status"

    local child_pid
    child_pid=$(cat "$childfile" 2>/dev/null || true)
    [ -n "$child_pid" ] || fail "the early-exit stub did not record its child pid"
    if kill -0 "$child_pid" 2>/dev/null; then
        fail "a child survived after its successful parent exited"
    fi
}

# A timed-out parent and child can both ignore TERM. The supervisor must retain
# the leader until KILL and reap both processes, while preserving timeout 124.
self_test_bounded_command_kills_term_ignoring_descendants() {
    local stub=$SMOKE_ROOT/term-ignoring-stub.sh
    local pidfile=$SMOKE_ROOT/stub.pid
    local childfile=$SMOKE_ROOT/stub-child.pid
    cat >"$stub" <<'STUB'
#!/bin/sh
trap '' TERM
printf '%s\n' "$$" >"$1"
(trap '' TERM; sleep 300) &
printf '%s\n' "$!" >"$2"
wait
STUB
    chmod 755 "$stub"

    # Shorten the grace period for this check only; the assignment stays inside
    # the check subshell.
    KILL_GRACE_SECONDS=1
    local status=0
    set +e
    run_bounded 1 "$LOG_DIR/stub-out.txt" "$LOG_DIR/stub-err.txt" \
        /bin/sh "$stub" "$pidfile" "$childfile"
    status=$?
    set -e
    [ "$status" = "124" ] ||
        fail "a command that ignored TERM returned status $status instead of timeout 124"

    local stub_pid child_pid polls=0
    stub_pid=$(cat "$pidfile" 2>/dev/null || true)
    child_pid=$(cat "$childfile" 2>/dev/null || true)
    [ -n "$stub_pid" ] || fail "the stub did not record its own pid"
    [ -n "$child_pid" ] || fail "the stub did not record its child pid"
    while ((polls < 25)); do
        if ! kill -0 "$stub_pid" 2>/dev/null && ! kill -0 "$child_pid" 2>/dev/null; then
            break
        fi
        sleep 0.2
        polls=$((polls + 1))
    done
    if kill -0 "$stub_pid" 2>/dev/null; then
        fail "the TERM-ignoring command survived the bound"
    fi
    if kill -0 "$child_pid" 2>/dev/null; then
        fail "a descendant of the bounded command survived"
    fi
}

# The validator must reject documents that are malformed, empty, or leak a
# private path, and accept the one the writer just produced.
self_test_evidence_validator_rejects_bad_documents() {
    local dir=$SMOKE_ROOT/validator
    mkdir -p "$dir"
    local good=$dir/good.json
    local saved_file=$EVIDENCE_FILE
    EVIDENCE_FILE=$good
    emit_evidence force || fail "the evidence writer could not produce a document"
    EVIDENCE_FILE=$saved_file
    validate_evidence "$good" ||
        fail "a freshly written document failed validation: $EVIDENCE_ERROR"

    printf 'not json at all\n' >"$dir/plain.json"
    if validate_evidence "$dir/plain.json"; then
        fail "the validator accepted a document that is not JSON"
    fi
    printf '{"schema_version": "%s"}\n' "$EVIDENCE_SCHEMA" >"$dir/partial.json"
    if validate_evidence "$dir/partial.json"; then
        fail "the validator accepted a document with missing fields"
    fi
    : >"$dir/empty.json"
    if validate_evidence "$dir/empty.json"; then
        fail "the validator accepted an empty document"
    fi
    if [ -n "$REAL_HOME" ]; then
        sed "s#\"mode\": \"self-test\"#\"mode\": \"self-test$REAL_HOME\"#" \
            "$good" >"$dir/leaky.json"
        if validate_evidence "$dir/leaky.json"; then
            fail "the validator accepted a document containing a home directory path"
        fi
    fi
}

# Regression guard for toolchain selection. The fixture pins a channel no host
# would default to, so this only passes when the pin is read from the checkout and
# exported explicitly. If selection ever came from rustup walking up to a
# rust-toolchain.toml, or from the host default, the value below could not appear.
# The check body runs in a subshell, so reassigning SOURCE_DIR cannot leak.
self_test_pinned_toolchain_is_selected() {
    local fixture=$SMOKE_ROOT/fixture-checkout
    mkdir -p "$fixture"
    printf '[toolchain]\nchannel = "0.0.0-fixture"\ncomponents = ["rustfmt"]\n' \
        >"$fixture/rust-toolchain.toml"
    printf '[workspace]\nmembers = []\n' >"$fixture/Cargo.toml"

    SOURCE_DIR=$fixture
    PINNED_TOOLCHAIN=""
    read_pinned_toolchain
    [ "$PINNED_TOOLCHAIN" = "0.0.0-fixture" ] ||
        fail "the pin was not read from rust-toolchain.toml"

    configure_build_env
    local entry found=""
    for entry in "${BUILD_ENV[@]}"; do
        case $entry in
            RUSTUP_TOOLCHAIN=*) found=${entry#RUSTUP_TOOLCHAIN=} ;;
        esac
    done
    [ "$found" = "0.0.0-fixture" ] ||
        fail "the build environment selects '$found' rather than the pinned channel"
}

# A long-running bounded check used only by --signal-self-test. It publishes what
# an external test needs in order to signal the top-level process, then waits to
# be interrupted.
signal_self_test_long_check() {
    local stub=$SMOKE_ROOT/signal-stub.sh
    local pidfile=$SMOKE_ROOT/signal-stub.pid
    local childfile=$SMOKE_ROOT/signal-stub-child.pid
    cat >"$stub" <<'STUB'
#!/bin/sh
trap '' TERM
mark_identity() {
    identity_pid=$1
    identity_path=$2
    stat=$(cat "/proc/$identity_pid/stat") || return 1
    fields=${stat##*) }
    set -- $fields
    shift 19
    printf '%s %s\n' "$identity_pid" "$1" >"$identity_path.tmp"
    mv -- "$identity_path.tmp" "$identity_path"
}
printf '%s\n' "$$" >"$1"
mark_identity "$$" "$5/signal-stub-identity.pid"
sleep 300 &
child_pid=$!
printf '%s\n' "$child_pid" >"$2"
mark_identity "$child_pid" "$5/signal-child-identity.pid"
{
    printf 'top_pid=%s\n' "$3"
    printf 'stub_pid_file=%s\n' "$1"
    printf 'child_pid_file=%s\n' "$2"
} >"$4"
wait
STUB
    chmod 755 "$stub"
    # $$ is the top-level script pid even inside the check subshell. The stub
    # writes the marker only after both pids exist, so the external test cannot
    # signal before there is something to clean up.
    run_bounded 120 "$LOG_DIR/signal-out.txt" "$LOG_DIR/signal-err.txt" \
        /bin/sh "$stub" "$pidfile" "$childfile" "$$" "$EVIDENCE_DIR/signal-test-ready" \
        "$EVIDENCE_DIR"
    fail "the long-running check finished instead of being interrupted"
}

self_test_evidence_fails_closed() {
    local scratch=$SMOKE_ROOT/scratch-evidence.json
    local saved_file=$EVIDENCE_FILE
    # The expected count depends on the checks that ran before this one, so a
    # genuine failure elsewhere does not make this assertion meaningless.
    local expected_failed
    expected_failed=$(( $(count_failures) + 1 ))
    EVIDENCE_FILE=$scratch
    CHECK_NAMES+=("synthetic-failure")
    CHECK_STATUSES+=("fail")
    CHECK_DURATIONS+=(0)
    emit_evidence force || fail "the evidence writer could not produce a document"
    EVIDENCE_FILE=$saved_file
    drop_last CHECK_NAMES
    drop_last CHECK_STATUSES
    drop_last CHECK_DURATIONS
    expect_file "$scratch" "the scratch evidence file"
    local body
    body=$(cat "$scratch")
    expect_contains "$body" "\"schema_version\": \"$EVIDENCE_SCHEMA\"" "the scratch evidence"
    expect_contains "$body" '"result": "fail"' "the scratch evidence"
    expect_contains "$body" "\"failed\": $expected_failed," "the scratch evidence"
    expect_contains "$body" '"synthetic-failure"' "the scratch evidence"
    rm -f "$scratch"
}

self_test_evidence_contract() {
    emit_evidence force || fail "the evidence writer could not produce a document"
    expect_file "$EVIDENCE_FILE" "the evidence file"
    local body expected_result="pass"
    body=$(cat "$EVIDENCE_FILE")
    if (( $(count_failures) > 0 )); then expected_result="fail"; fi
    expect_contains "$body" "\"schema_version\": \"$EVIDENCE_SCHEMA\"" "the evidence"
    expect_contains "$body" '"mode": "self-test"' "the evidence"
    # The result must agree with the recorded counts rather than being assumed.
    expect_contains "$body" "\"result\": \"$expected_result\"" "the evidence"
    expect_contains "$body" "\"failed\": $(count_failures)," "the evidence"
    expect_absent "$body" "$SENTINEL" "the evidence"
    expect_absent "$body" "$REAL_HOME" "the evidence"
    expect_absent "$body" "$SOURCE_DIR" "the evidence"
    expect_no_user_name "$body" "the evidence"
}

run_self_test() {
    RUN_MODE="self-test"
    # Install the trap before any temporary directory exists, so an early fatal
    # error cannot leak one. cleanup tolerates an uninitialized root.
    install_traps
    require_process_supervisor
    create_isolated_roots
    record_identity
    BUILD_ENV=("${ISOLATED_ENV[@]}")

    echo "Urdia smoke self-test (no build, no agent, no network)"
    echo "  commit:   $EVIDENCE_COMMIT (dirty: $EVIDENCE_DIRTY)"
    echo "  evidence: $EVIDENCE_DIR"

    run_check "self-test-sanitizer-redacts-private-values" self_test_sanitizer
    run_check "self-test-diagnostics-are-bounded-and-escaped" self_test_bounds
    run_check "self-test-failure-log-is-sanitized" self_test_failure_log_sanitized
    run_check "self-test-first-failure-wins" self_test_first_failure_wins
    run_check "self-test-installed-binary-is-selected" self_test_installed_binary_is_selected
    run_check "self-test-pinned-toolchain-is-selected" self_test_pinned_toolchain_is_selected
    run_check "self-test-bounded-command-cleans-after-successful-parent-exit" \
        self_test_bounded_command_cleans_after_successful_parent_exit
    run_check "self-test-bounded-command-kills-term-ignoring-descendants" \
        self_test_bounded_command_kills_term_ignoring_descendants
    run_check "self-test-evidence-validator-rejects-bad-documents" \
        self_test_evidence_validator_rejects_bad_documents
    run_check "self-test-evidence-fails-closed" self_test_evidence_fails_closed
    run_check "self-test-evidence-contract" self_test_evidence_contract

    finish_run
}

# Publishes the evidence and turns any failure into a nonzero exit. Evidence is
# mandatory: a run that cannot publish it did not succeed, whatever its checks
# reported.
finish_run() {
    RUN_COMPLETED=1
    local status=0 failed
    if ! emit_evidence force; then
        echo "smoke: ${EVIDENCE_ERROR:-could not write the evidence file}" >&2
        status=1
    fi
    failed=$(count_failures)
    echo "  checks:   ${#CHECK_NAMES[@]} recorded, $failed failed"
    if ((failed > 0)); then
        echo "  diagnostics: $FAILURES_LOG" >&2
        status=1
    fi
    return "$status"
}

# ---------------------------------------------------------------------------
# Full mode
# ---------------------------------------------------------------------------

run_full_smoke() {
    install_traps
    require_process_supervisor
    create_isolated_roots
    record_identity
    configure_build_env
    record_toolchain

    # Not called "offline": the build phase may fetch public crates. Every check
    # after it is offline.
    echo "Urdia release smoke test"
    echo "  commit:   $EVIDENCE_COMMIT (dirty: $EVIDENCE_DIRTY)"
    echo "  rustc:    $EVIDENCE_RUSTC"
    echo "  target:   $TARGET_DIR"
    echo "  cargo:    $EFFECTIVE_CARGO_HOME"
    echo "  evidence: $EVIDENCE_DIR"

    if [ "$SKIP_BUILD" = "1" ]; then
        require_check "prebuilt-binaries-present" check_prebuilt_binaries
    else
        [ "$TOOLCHAIN_AVAILABLE" = "1" ] ||
            fatal "no Rust toolchain was found; install one or pass --skip-build"
        require_check "release-build-locked" check_release_build
        require_check "lockfile-unchanged" check_lockfile_unchanged
    fi
    if [ "$TOOLCHAIN_AVAILABLE" = "1" ]; then
        run_check "toolchain-recorded" check_toolchain_recorded
    fi

    require_check "environment-isolation" check_environment_isolation
    require_check "install-binary" check_install_binary
    # Runs in the parent shell: from here on every CLI check executes the
    # installed copy rather than the build tree.
    use_installed_binary
    require_check "installed-binary-exercised" check_installed_binary_exercised

    run_check "version-report" check_version_report
    run_check "doctor" check_doctor
    run_check "providers-listing" check_providers_listing
    run_check "external-provider-registration" check_external_provider_registration
    run_check "streamed-run" check_streamed_run
    run_check "content-retention-positive-control" check_content_retention_positive_control
    run_check "default-run-retains-no-content" check_default_run_retains_no_content
    run_check "deny-by-default-permission" check_deny_by_default_permission
    run_check "timeline-export" check_timeline_export
    run_check "deterministic-route" check_deterministic_route
    run_check "deterministic-eval-routing" check_deterministic_eval_routing
    run_check "non-loopback-bind-rejected" check_non_loopback_bind_rejected
    run_check "decision-providers-unconfigured" check_decision_providers_unconfigured
    run_check "uninstall" check_uninstall
    run_check "evidence-sanitized" check_evidence_sanitized

    finish_run
}

# Exists for the external interrupt regression test: one long-running bounded
# check, then wait to be signalled. The test signals only the top-level pid and
# asserts a prompt exit with no surviving descendants.
run_signal_self_test() {
    RUN_MODE="signal-self-test"
    install_traps
    require_process_supervisor
    create_isolated_roots
    record_identity
    BUILD_ENV=("${ISOLATED_ENV[@]}")

    echo "Urdia smoke signal self-test (waits to be signalled)"
    echo "  evidence: $EVIDENCE_DIR"
    printf '%s\n' "ready" >"$EVIDENCE_DIR/signal-test-top-ready"

    run_check "signal-self-test-long-running-check" signal_self_test_long_check

    finish_run
}

RUN_START_MS=$(now_ms)
if [ "$SELF_TEST" = "1" ]; then
    run_self_test
elif [ "$SIGNAL_SELF_TEST" = "1" ]; then
    run_signal_self_test
else
    run_full_smoke
fi
