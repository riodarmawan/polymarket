#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - "$@" <<'PY'
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path.cwd()
EXCLUDED_PREFIXES = (
    ".git/",
    "data-production/",
    "node_modules/",
    "target/",
    "polymarket-bot/target/",
)
EXCLUDED_FILES = {
    "deploy/scan-repo-secrets.sh",
}
EXCLUDED_SUFFIXES = (
    ".cdx.json",
    ".sqlite",
    ".db",
)
ALLOW_MARKERS = (
    "replace_me",
    "0xreplace_me",
    "your_",
    "example",
    "placeholder",
    "redacted",
    "[redacted]",
    "<client_key>",
    "<authorization_id>",
    "<intent_amount>",
    "disabled",
)

PATTERNS = [
    (
        "ethereum_private_key",
        re.compile(r"\b0x[a-fA-F0-9]{64}\b"),
    ),
    (
        "sensitive_env_assignment",
        re.compile(
            r"\b(?:POLYMARKET_)?(?:PRIVATE_KEY|MNEMONIC|SEED_PHRASE|CLOB_API_SECRET|CLOB_PASSPHRASE|RELAYER_API_KEY)\b\s*[:=]\s*[`'\"]?([^`'\"\s#]+)"
        ),
    ),
    (
        "markdown_secret_value",
        re.compile(
            r"(?i)\b(?:private key|mnemonic|seed phrase|api secret|passphrase|relayer api key)\b[^`]{0,80}`([^`]+)`"
        ),
    ),
    (
        "mnemonic_assignment",
        re.compile(
            r"(?i)\b(?:mnemonic|seed phrase)\b\s*[:=|]\s*`?([a-z]+(?:\s+[a-z]+){11,})`?"
        ),
    ),
]


def candidate_files() -> list[str]:
    output = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
    )
    return [item for item in output.decode().split("\0") if item]


def is_excluded(path: str) -> bool:
    return (
        path in EXCLUDED_FILES
        or path.startswith(EXCLUDED_PREFIXES)
        or path.endswith(EXCLUDED_SUFFIXES)
    )


def is_allowed(line: str) -> bool:
    lowered = line.lower()
    return any(marker in lowered for marker in ALLOW_MARKERS)


def line_findings(path: str, line_number: int, line: str) -> list[tuple[str, int, str, str]]:
    if is_allowed(line):
        return []
    return [
        (path, line_number, name, line.strip()[:160])
        for name, pattern in PATTERNS
        if pattern.search(line)
    ]


if "--self-test" in sys.argv:
    positive_lines = [
        "POLYMARKET_PRIVATE_KEY=0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "POLYMARKET_CLOB_API_SECRET=real-secret-value",
        "Mnemonic `alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima`",
    ]
    negative_lines = [
        "POLYMARKET_PRIVATE_KEY=replace_me",
        "POLYMARKET_CLOB_API_SECRET=replace_me",
        "Private key value is [REDACTED]",
    ]
    missed = [line for line in positive_lines if not line_findings("self-test", 1, line)]
    false_positive = [line for line in negative_lines if line_findings("self-test", 1, line)]
    if missed or false_positive:
        if missed:
            print(f"Self-test missed secret pattern(s): {missed}", file=sys.stderr)
        if false_positive:
            print(f"Self-test flagged allowed placeholder(s): {false_positive}", file=sys.stderr)
        sys.exit(1)
    print("PASS: repository secret scan self-test")
    sys.exit(0)


def is_text(path: Path) -> bool:
    try:
        with path.open("rb") as handle:
            sample = handle.read(4096)
        return b"\0" not in sample
    except OSError:
        return False


findings: list[tuple[str, int, str, str]] = []
for relative in candidate_files():
    if is_excluded(relative):
        continue
    path = ROOT / relative
    if not path.is_file() or not is_text(path):
        continue
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        continue
    for line_number, line in enumerate(lines, start=1):
        findings.extend(line_findings(relative, line_number, line))

if findings:
    print("Secret scan failed. Potential committed secret(s) found:", file=sys.stderr)
    for path, line_number, name, preview in findings:
        print(f"  {path}:{line_number}: {name}: {preview}", file=sys.stderr)
    sys.exit(1)

print("PASS: repository secret scan found no private keys, mnemonics, or API secrets")
PY
