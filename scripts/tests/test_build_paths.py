import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import build_paths


class BuildPathsTests(unittest.TestCase):
    def test_uses_cargo_output_instead_of_hard_coded_target(self):
        directory = Path('C:/custom build/cache')
        result = SimpleNamespace(stdout=json.dumps({'target_directory': str(directory)}))
        with patch.object(build_paths.subprocess, 'run', return_value=result):
            self.assertEqual(build_paths.release_executable(), directory / 'release' / 'scry.exe')

    def test_does_not_fall_back_to_stale_executable_on_metadata_failure(self):
        error = build_paths.subprocess.CalledProcessError(1, 'cargo')
        with patch.object(build_paths.subprocess, 'run', side_effect=error):
            with self.assertRaises(build_paths.subprocess.CalledProcessError):
                build_paths.release_executable()


if __name__ == '__main__':
    unittest.main()
