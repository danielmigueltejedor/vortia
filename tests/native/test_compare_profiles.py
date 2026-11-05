#!/usr/bin/env python3
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "benchmarks" / "common"))
from compare_profiles import load_xy


class LoadXyTests(unittest.TestCase):
    def test_accepts_header_before_numeric_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.csv"
            path.write_text("x,y\n0,1\n1,2\n", encoding="utf-8")

            self.assertEqual(load_xy(path), [(0.0, 1.0), (1.0, 2.0)])

    def test_rejects_malformed_row_after_numeric_data(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.csv"
            path.write_text("0,1\nnot-a-number,2\n1,2\n", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "malformed numeric row at line 2"):
                load_xy(path)

    def test_rejects_short_row_after_numeric_data(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "profile.csv"
            path.write_text("0,1\nmalformed\n1,2\n", encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "malformed row at line 2"):
                load_xy(path)

    def test_rejects_relative_path_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / "secret.csv"
            outside.write_text("0,1\n1,2\n", encoding="utf-8")
            nested = root / "nested"
            nested.mkdir()
            escape = Path("..") / "secret.csv"
            previous = Path.cwd()
            try:
                import os

                os.chdir(nested)
                with self.assertRaisesRegex(ValueError, "outside the working directory"):
                    load_xy(escape)
            finally:
                os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
