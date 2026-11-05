import contextlib
import io
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from compare_profiles import main, metrics


class CompareProfilesTest(unittest.TestCase):
    def test_zero_reference_with_nonzero_error_has_infinite_relative_error(self):
        _, _, rel_l2, rel_linf = metrics([0.0, 0.0], [1.0, 1.0])

        self.assertEqual(rel_l2, float("inf"))
        self.assertEqual(rel_linf, float("inf"))

    def test_zero_reference_exact_match_has_zero_relative_error(self):
        _, _, rel_l2, rel_linf = metrics([0.0, 0.0], [0.0, 0.0])

        self.assertEqual(rel_l2, 0.0)
        self.assertEqual(rel_linf, 0.0)

    def test_zero_reference_with_nonzero_candidate_fails_comparison(self):
        with tempfile.TemporaryDirectory() as directory:
            reference = Path(directory) / "reference.csv"
            candidate = Path(directory) / "candidate.csv"
            reference.write_text("0,0\n1,0\n", encoding="utf-8")
            candidate.write_text("0,1\n1,1\n", encoding="utf-8")
            args = [
                "compare_profiles.py",
                "--reference", str(reference),
                "--candidate", str(candidate),
                "--rel-l2-max", "10",
                "--rel-linf-max", "10",
            ]
            with patch.object(sys, "argv", args), contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(main(), 1)


SCRIPT = Path(__file__).with_name("compare_profiles.py")


class CompareProfilesFiniteInputTest(unittest.TestCase):
    def run_comparison(self, reference, candidate, *options):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            reference_path = root / "reference.csv"
            candidate_path = root / "candidate.csv"
            reference_path.write_text(reference)
            candidate_path.write_text(candidate)
            return subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--reference",
                    str(reference_path),
                    "--candidate",
                    str(candidate_path),
                    "--rel-l2-max",
                    "1",
                    "--rel-linf-max",
                    "1",
                    *options,
                ],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_finite_profiles_still_pass(self):
        result = self.run_comparison("0 1\n1 2\n", "0 1\n1 2\n")
        self.assertEqual(result.returncode, 0)
        self.assertIn("PASS", result.stdout)

    def test_non_finite_profiles_scales_thresholds_and_metrics_fail(self):
        cases = [
            ("nan 1\n1 2\n", "0 1\n1 2\n", ()),
            ("0 nan\n1 2\n", "0 1\n1 2\n", ()),
            ("0 1\n1 2\n", "0 1\ninf 2\n", ()),
            ("0 1\n1 2\n", "0 1\n1 inf\n", ()),
            ("0 1\n1 2\n", "0 1\n1 2\n", ("--scale-candidate", "nan")),
            ("0 1\n1 2\n", "0 1\n1 2\n", ("--scale-abscissa", "inf")),
            ("0 1\n1 2\n", "0 1\n1 2\n", ("--rel-l2-max", "nan")),
            ("0 1\n1 2\n", "0 1\n1 2\n", ("--rel-linf-max", "inf")),
            ("0 1\n1 2\n", "0 1e308\n1 1e308\n", ()),
        ]
        for reference, candidate, options in cases:
            with self.subTest(reference=reference, candidate=candidate, options=options):
                result = self.run_comparison(reference, candidate, *options)
                self.assertEqual(result.returncode, 1)
                self.assertNotIn("PASS", result.stdout)


if __name__ == "__main__":
    unittest.main()
