"""Focused contract checks for the ANUGA compatibility scaffold."""

from __future__ import annotations

import importlib.util
import json
import math
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


RUNNER_PATH = Path(__file__).with_name("run.py")
SPEC = importlib.util.spec_from_file_location("anuga_sidecar_runner", RUNNER_PATH)
assert SPEC is not None and SPEC.loader is not None
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class AnugaScaffoldContractTests(unittest.TestCase):
    def test_demo_streams_progressive_hydrograph_samples(self) -> None:
        with tempfile.TemporaryDirectory(prefix="instasplatter-anuga-demo-contract-") as temp:
            output_dir = Path(temp)
            emitted: list[dict[str, object]] = []
            request = {
                "runId": "demo-contract-run",
                "outputDir": str(output_dir),
                "extent": {"boundsEnu": [0.0, 0.0, 100.0, 80.0]},
                "scenario": {"id": "demo-contract-scenario", "durationHours": 12.0},
            }

            with (
                patch.object(runner, "emit", side_effect=emitted.append),
                patch.object(runner.time, "sleep"),
            ):
                runner.run_demo(request)

            hydrograph_events = [event for event in emitted if event.get("kind") == "hydrograph"]
            self.assertTrue(hydrograph_events, "the live chart needs progressive hydrograph samples")
            sample_windows = [event["samples"] for event in hydrograph_events]
            self.assertTrue(all(len(a) < len(b) for a, b in zip(sample_windows, sample_windows[1:])))
            self.assertGreaterEqual(len(sample_windows[0]), 2)
            self.assertEqual(sample_windows[-1][-1]["hours"], 12.0)
            self.assertTrue(
                all(
                    math.isfinite(float(sample[field]))
                    for window in sample_windows
                    for sample in window
                    for field in ("hours", "stageM", "dischargeCms")
                )
            )

    def test_completed_flat_bed_evolve_is_never_reported_as_anuga(self) -> None:
        with tempfile.TemporaryDirectory(prefix="instasplatter-anuga-contract-") as temp:
            output_dir = Path(temp)
            emitted: list[dict[str, object]] = []
            request = {
                "runId": "contract-run",
                "outputDir": str(output_dir),
                "extent": {
                    "boundsEnu": [0.0, 0.0, 100.0, 80.0],
                    "meshMaxAreaM2": 25.0,
                },
                "scenario": {"id": "contract-scenario", "durationHours": 1.0},
            }

            with (
                patch.object(
                    runner,
                    "try_domain_evolve",
                    return_value={"max_depth": 1.2, "mean_depth": 0.4, "evolved": True},
                ),
                patch.object(runner, "emit", side_effect=emitted.append),
                patch.object(runner.time, "sleep"),
            ):
                runner.run_anuga(request, object(), "contract-version")

            done = next(event for event in emitted if event.get("kind") == "done")
            self.assertEqual(done["mode"], "scaffold")
            self.assertIn("not authoritative", str(done["label"]).lower())
            hydrograph_events = [event for event in emitted if event.get("kind") == "hydrograph"]
            self.assertTrue(hydrograph_events, "the live chart needs progressive hydrograph samples")
            sample_windows = [event["samples"] for event in hydrograph_events]
            self.assertTrue(all(len(a) < len(b) for a, b in zip(sample_windows, sample_windows[1:])))
            self.assertGreaterEqual(len(sample_windows[0]), 2)
            self.assertEqual(sample_windows[-1][-1]["hours"], 1.0)
            self.assertTrue(
                all(
                    math.isfinite(float(sample[field]))
                    for window in sample_windows
                    for sample in window
                    for field in ("hours", "stageM", "dischargeCms")
                )
            )
            self.assertTrue(
                all(
                    event.get("mode") == "scaffold"
                    for event in emitted
                    if event.get("kind") in {"progress", "done"}
                )
            )

            manifest = json.loads((output_dir / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["mode"], "scaffold")
            self.assertFalse(manifest["authoritative"])
            self.assertTrue(manifest["flatBedEvolveSmokeCompleted"])


if __name__ == "__main__":
    unittest.main()
