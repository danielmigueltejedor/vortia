#!/usr/bin/env python3
import math
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "benchmarks" / "common" / "gci.py"
sys.path.insert(0, str(ROOT / "benchmarks" / "common"))
from gci import compute_gci  # noqa: E402


class GciLibraryTests(unittest.TestCase):
    def test_exact_second_order(self):
        result = compute_gci(
            f1=1.0 + 0.25**2,
            f2=1.0 + 0.5**2,
            f3=1.0 + 1.0**2,
            r21=2.0,
            r32=2.0,
        )
        self.assertAlmostEqual(result["observed_order"], 2.0, places=9)
        self.assertAlmostEqual(result["extrapolated"], 1.0, places=9)
        self.assertAlmostEqual(result["asymptotic_ratio"], 1.0, places=6)

    def test_rejects_bad_ratio(self):
        with self.assertRaises(ValueError):
            compute_gci(1.0, 1.1, 1.2, r21=1.0, r32=2.0)


class GciCliTests(unittest.TestCase):
    def test_cli_prints_order(self):
        completed = subprocess.run(
            [
                sys.executable,
                str(SCRIPT),
                "--f1",
                str(1.0 + 0.25**2),
                "--f2",
                str(1.0 + 0.5**2),
                "--f3",
                str(1.0 + 1.0**2),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("p=", completed.stdout)
        self.assertTrue(math.isfinite(float(completed.stdout.split("p=")[1].split()[0])))


if __name__ == "__main__":
    unittest.main()
