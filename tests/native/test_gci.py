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
        f1 = 1.0 + 0.25**2
        f2 = 1.0 + 0.5**2
        # Equal-r relative GCIs ⇒ asym ≈ |f1|/|f2|, not an independent check.
        self.assertAlmostEqual(result["asymptotic_ratio"], abs(f1) / abs(f2), places=6)

    def test_unequal_refinement_second_order(self):
        result = compute_gci(
            f1=1.0 + 1.0**2,
            f2=1.0 + 2.0**2,
            f3=1.0 + 6.0**2,
            r21=2.0,
            r32=3.0,
        )
        self.assertAlmostEqual(result["observed_order"], 2.0, places=6)
        self.assertAlmostEqual(result["extrapolated"], 1.0, places=6)

    def test_unequal_refinement_smaller_coarse_ratio(self):
        result = compute_gci(
            f1=1.0 + 1.0**2,
            f2=1.0 + 2.0**2,
            f3=1.0 + 2.2**2,
            r21=2.0,
            r32=1.1,
        )
        self.assertAlmostEqual(result["observed_order"], 2.0, places=5)
        self.assertAlmostEqual(result["extrapolated"], 1.0, places=5)

    def test_rejects_oscillatory(self):
        with self.assertRaisesRegex(ValueError, "oscillatory"):
            compute_gci(1.0, 2.0, 0.0, r21=2.0, r32=2.0)

    def test_rejects_tiny_oscillatory(self):
        with self.assertRaisesRegex(ValueError, "oscillatory"):
            compute_gci(1e-170, 2e-170, -2e-170, r21=2.0, r32=2.0)

    def test_identical_meshes(self):
        result = compute_gci(42.0, 42.0, 42.0, r21=2.0, r32=2.0)
        self.assertEqual(result["gci12"], 0.0)
        self.assertEqual(result["extrapolated"], 42.0)

    def test_signed_zeros_identical(self):
        result = compute_gci(0.0, -0.0, 0.0, r21=2.0, r32=2.0)
        self.assertEqual(result["gci12"], 0.0)

    def test_signed_zero_difference_is_zero(self):
        with self.assertRaisesRegex(ValueError, "zero consecutive"):
            compute_gci(0.0, -0.0, -1.0, r21=2.0, r32=2.0)

    def test_rejects_equal_ratio_divergent(self):
        with self.assertRaisesRegex(ValueError, "equal-ratio"):
            compute_gci(1.0, 2.0, 2.5, r21=2.0, r32=2.0)

    def test_rejects_equal_ratio_errors_growing_toward_fine(self):
        with self.assertRaisesRegex(ValueError, "equal-ratio"):
            compute_gci(100.02, 100.01, 100.005, r21=2.0, r32=2.0)

    def test_rejects_unequal_ratio_without_positive_order(self):
        # |ε32/ε21|=0.1 < ln(1.3)/ln(2) ≈ 0.379 ⇒ no positive-order fit.
        with self.assertRaisesRegex(ValueError, "unequal-ratio"):
            compute_gci(1.0, 2.0, 2.1, r21=2.0, r32=1.3)

    def test_zero_consecutive_difference(self):
        with self.assertRaisesRegex(ValueError, "zero consecutive"):
            compute_gci(1.0, 1.0, 2.0, r21=2.0, r32=2.0)

    def test_rejects_bad_ratio(self):
        with self.assertRaises(ValueError):
            compute_gci(1.0, 1.1, 1.2, r21=1.0, r32=2.0)

    def test_small_magnitude_second_order(self):
        result = compute_gci(
            f1=1e-16 + 1e-16 * 0.25**2,
            f2=1e-16 + 1e-16 * 0.5**2,
            f3=1e-16 + 1e-16 * 1.0**2,
            r21=2.0,
            r32=2.0,
        )
        self.assertAlmostEqual(result["observed_order"], 2.0, places=6)


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

    def test_cli_rejects_invalid(self):
        completed = subprocess.run(
            [sys.executable, str(SCRIPT), "--f1", "1", "--f2", "1", "--f3", "2"],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(completed.returncode, 1)
        self.assertIn("ERROR:", completed.stderr)


if __name__ == "__main__":
    unittest.main()
