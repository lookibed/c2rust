#!/usr/bin/env python3
"""Build and verify the frozen c2das fixture/runner inventory.

The catalog is intentionally declarative.  This script is not a test runner:
it makes unregistered source fixtures and unreviewed runner entrypoints a
review failure before they can become another parallel testing system.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "tests/registry/catalog.json"
OUTPUT = ROOT / "tests/registry/fixtures.json"
CANONICAL_CASES = ROOT / "tests/canonical/cases.json"
VALID_STATUSES = {
    "ast-green",
    "blocked",
    "historical",
    "inventory-only",
    "known-red",
    "quarantined",
    "retained-non-e2e",
    "retire",
    "supported",
}
RUNNER_GLOBS = (
    ".github/workflows/*.yml",
    "scripts/c2das_preflight.*",
    "scripts/check_*.py",
    "scripts/run_c2das_*.py",
    "scripts/*test*.py",
    "scripts/run_ci_checks.sh",
    "tests/integration/test.py",
    "tests/syntax/check_*.sh",
    "tests/run_*.ps1",
    "tests/run_*.bat",
    "tests/manual/*/check_*.sh",
    "tests/manual/*/run_*.sh",
    "tests/manual/*/run_*.ps1",
    "tests/manual/*/transpile_*.sh",
)


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def load_catalog() -> dict:
    return json.loads(CATALOG.read_text(encoding="utf-8"))


def load_canonical_cases() -> dict[str, dict]:
    document = json.loads(CANONICAL_CASES.read_text(encoding="utf-8"))
    if document.get("schema_version") != 1 or not isinstance(document.get("cases"), list):
        raise ValueError("invalid canonical case manifest")
    identifiers = [case.get("id") for case in document["cases"]]
    if any(not isinstance(identifier, str) for identifier in identifiers):
        raise ValueError("canonical case without a string id")
    if len(identifiers) != len(set(identifiers)):
        raise ValueError("duplicate canonical case id")
    return {case["id"]: case for case in document["cases"]}


def derived_override(case: dict) -> dict:
    """Registry facts implied by a canonical case.

    The canonical manifest is the single source of truth for executable
    cases; the registry mirrors it instead of asking for a hand-written
    duplicate override per case."""
    status = case.get("status")
    if status == "negative":
        return {
            "canonical_case": case["id"],
            "entrypoint": "not-applicable",
            "expected_kind": "translation-error",
            "expected_value": case.get("expected_error", "unregistered"),
            "runtime": "not-applicable",
            "status": "supported",
        }
    expected = case.get("expected", {})
    kind = "c-reference-oracle" if expected.get("oracle") == "c-reference" else "stdout-and-exit-code"
    if case.get("expected_exporter_failure") is not None:
        kind = "exporter-signal"
        expected = case["expected_exporter_failure"]
    return {
        "canonical_case": case["id"],
        "entrypoint": case.get("das_entrypoint", "unregistered"),
        "expected_kind": kind,
        "expected_value": expected,
        "runtime": case.get("runtime", "unverified"),
        "status": "supported" if status == "ready" else "known-red",
    }


def records(catalog: dict) -> dict:
    graphs = {graph["id"]: graph for graph in catalog["graph"]}
    overrides = {item["path"]: item for item in catalog.get("override", [])}
    canonical_cases = load_canonical_cases()
    canonical_by_source = {
        f"{case['source_root'].rstrip('/')}/{case['translation_entry']}": case
        for case in canonical_cases.values()
    }
    seen: dict[str, str] = {}
    fixtures: list[dict] = []

    for family in catalog["family"]:
        graph_id = family["c_graph"]
        if graph_id not in graphs:
            raise ValueError(f"{family['id']}: unknown C graph {graph_id}")
        for source in sorted(ROOT.glob(family["glob"])):
            if not source.is_file():
                continue
            path = relative(source)
            if path in seen:
                raise ValueError(
                    f"fixture {path} belongs to both {seen[path]} and {family['id']}"
                )
            seen[path] = family["id"]
            override = overrides.get(path)
            if override is None and path in canonical_by_source:
                override = derived_override(canonical_by_source[path])
            override = override or {}
            entrypoint = override.get("entrypoint", family["entrypoint"])
            fixture = {
                "id": path.removesuffix(".c").replace("/", "--"),
                "source": path,
                "family": family["id"],
                "c_graph": graph_id,
                "clang": {
                    "flags": graphs[graph_id].get("clang_flags", ["from-compile-commands"]),
                    "compile_commands": graphs[graph_id].get("compile_commands"),
                },
                "owner": override.get("owner", family["owner"]),
                "entrypoint": entrypoint,
                "expected": {
                    "kind": override.get("expected_kind", family["expected_kind"]),
                    "value": override.get("expected_value", "unregistered"),
                },
                "checked_in_das": (
                    relative(source.with_suffix(".das"))
                    if source.with_suffix(".das").is_file()
                    else None
                ),
                "runtime": override.get("runtime", family["runtime"]),
                "status": override.get("status", family["status"]),
            }
            if "canonical_case" in override:
                fixture["canonical_case"] = override["canonical_case"]
            fixtures.append(fixture)

    audited_roots = ("tests/syntax", "tests/unit", "tests/manual", "tests/production", "c2dascript-transpile/tests/snapshots")
    unregistered = []
    for root in audited_roots:
        for source in (ROOT / root).rglob("*.c"):
            path = relative(source)
            if "/upstream/" not in f"/{path}" and path not in seen:
                unregistered.append(path)
    if unregistered:
        raise ValueError("unregistered C fixtures: " + ", ".join(sorted(unregistered)))

    for fixture in fixtures:
        if fixture["status"] not in VALID_STATUSES:
            raise ValueError(f"{fixture['source']}: unknown status {fixture['status']}")
        if not fixture["owner"] or not fixture["c_graph"]:
            raise ValueError(f"{fixture['source']}: owner and C graph are required")
        if fixture["entrypoint"] == "unregistered" and fixture["status"] not in {
            "historical", "inventory-only", "quarantined"
        }:
            raise ValueError(f"{fixture['source']}: executable claim lacks an entrypoint")
        if fixture["status"] == "supported":
            if fixture["expected"]["value"] == "unregistered":
                raise ValueError(f"{fixture['source']}: supported fixture lacks an exact expected value")
            if fixture.get("canonical_case") is None:
                raise ValueError(f"{fixture['source']}: supported fixture lacks a canonical case")
            if fixture["canonical_case"] not in canonical_cases:
                raise ValueError(
                    f"{fixture['source']}: missing canonical case {fixture['canonical_case']}"
                )

    fixture_by_case = {
        fixture["canonical_case"]: fixture
        for fixture in fixtures
        if "canonical_case" in fixture
    }
    for case_id, case in canonical_cases.items():
        fixture = fixture_by_case.get(case_id)
        if fixture is None:
            raise ValueError(f"{case_id}: canonical case has no registry fixture")
        source = f"{case['source_root'].rstrip('/')}/{case['translation_entry']}"
        if fixture["source"] != source:
            raise ValueError(f"{case_id}: manifest source disagrees with registry fixture")
        if fixture["c_graph"] != case["c_graph"]:
            raise ValueError(f"{case_id}: manifest C graph disagrees with registry fixture")
        exporter_failure = case.get("expected_exporter_failure")
        if exporter_failure is not None:
            if case.get("status") != "known-red" or fixture["status"] != "known-red":
                raise ValueError(f"{case_id}: exporter failure cannot be promoted to ready/supported")
            if fixture["expected"]["kind"] != "exporter-signal":
                raise ValueError(f"{case_id}: exporter failure has wrong registry expected kind")
            if fixture["expected"]["value"] != exporter_failure:
                raise ValueError(f"{case_id}: exporter failure contract disagrees with registry")
        if case.get("status") != "ready":
            continue
        if fixture["status"] != "supported":
            raise ValueError(f"{case_id}: ready canonical case has no supported registry fixture")
        if fixture["entrypoint"] != case["das_entrypoint"]:
            raise ValueError(f"{case_id}: manifest daScript entrypoint disagrees with registry fixture")
        if fixture["expected"]["value"] != case["expected"]:
            raise ValueError(f"{case_id}: manifest oracle disagrees with registry fixture")

    runner_paths = set()
    for runner in catalog["runner"]:
        path = runner["path"]
        if path in runner_paths:
            raise ValueError(f"runner registered twice: {path}")
        runner_paths.add(path)
        if not (ROOT / path).is_file():
            raise ValueError(f"runner does not exist: {path}")
        if runner["status"] not in VALID_STATUSES:
            raise ValueError(f"{path}: unknown runner status {runner['status']}")
        if not runner.get("role") or not runner.get("reason"):
            raise ValueError(f"{path}: runner role and reason are required")

    discovered_runners = set()
    for pattern in RUNNER_GLOBS:
        discovered_runners.update(
            relative(path)
            for path in ROOT.glob(pattern)
            if path.is_file() and "/upstream/" not in f"/{relative(path)}"
        )
    missing_runners = discovered_runners - runner_paths
    extra_runners = runner_paths - discovered_runners
    if missing_runners:
        raise ValueError("unregistered runners: " + ", ".join(sorted(missing_runners)))
    if extra_runners:
        raise ValueError("catalogued path is not a recognized runner: " + ", ".join(sorted(extra_runners)))

    return {
        "schema_version": catalog["schema_version"],
        "purpose": "Frozen inventory; statuses are facts, never a compatibility claim.",
        "graphs": catalog["graph"],
        "fixtures": sorted(fixtures, key=lambda item: item["source"]),
        "runners": sorted(catalog["runner"], key=lambda item: item["path"]),
    }


def render(document: dict) -> str:
    return json.dumps(document, indent=2, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--write", action="store_true")
    mode.add_argument("--list", action="store_true")
    args = parser.parse_args()

    try:
        document = records(load_catalog())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"test registry invalid: {error}", file=sys.stderr)
        return 1

    generated = render(document)
    if args.list:
        try:
            for fixture in document["fixtures"]:
                print(f"{fixture['status']:14} {fixture['owner']:40} {fixture['source']}")
        except BrokenPipeError:
            return 0
        return 0
    if args.write:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(generated, encoding="utf-8")
        return 0
    if not OUTPUT.is_file() or OUTPUT.read_text(encoding="utf-8") != generated:
        print("test registry drift: run python3 scripts/check_test_registry.py --write", file=sys.stderr)
        return 1
    print(
        f"test registry: {len(document['fixtures'])} fixtures, "
        f"{len(document['runners'])} runners, {len(document['graphs'])} C graphs"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
