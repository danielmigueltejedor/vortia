import contextlib
import io
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


if __name__ == "__main__":
    unittest.main()
