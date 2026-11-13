"""scripts/clean.py: which servers it stops, and how it is called."""

import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import clean

TEST_URL = b"DATABASE_URL=postgres://postgres@localhost:54329/t_1763050000_4242_7"


def environ(*variables):
    return b"\0".join(variables) + b"\0"


class CleanTest(unittest.TestCase):
    def setUp(self):
        self.proc = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(self.proc))
        self.temporary = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(self.temporary))

    def process(self, pid, name, exe, env):
        folder = self.proc / str(pid)
        folder.mkdir()
        (folder / "comm").write_text(f"{name}\n")
        (folder / "environ").write_bytes(env)
        (folder / "exe").symlink_to(exe)

    def test_only_a_test_database_marks_a_test_server(self):
        self.assertTrue(clean.is_test_server(environ(b"PORT=1", TEST_URL)))
        self.assertTrue(clean.is_test_server(environ(TEST_URL + b"?sslmode=disable")))
        for url in [
            b"postgres://localhost:54329/tetsu_local",
            b"postgres://h/t_2024_backup",
            b"postgres://h/t_1_2_3x",
        ]:
            self.assertFalse(clean.is_test_server(environ(b"DATABASE_URL=" + url)), url)
        self.assertFalse(clean.is_test_server(environ(b"OTHER_URL=postgres://h/t_1_2_3")))

    def test_built_here_includes_deleted_copies_and_nothing_outside_target(self):
        root = Path("/work/tetsu")
        self.assertTrue(clean.built_here("/work/tetsu/target/debug/tetsu", root))
        self.assertTrue(clean.built_here("/work/tetsu/target/mutants-tmp/x/tetsu (deleted)", root))
        self.assertFalse(clean.built_here("/usr/local/bin/tetsu", root))
        self.assertFalse(clean.built_here("/work/tetsu/targets/tetsu", root))

    def test_test_servers_are_stopped_and_a_server_started_by_hand_is_not(self):
        self.process(10, "tetsu", "/work/tetsu/target/debug/tetsu", environ(TEST_URL))
        self.process(
            11, "tetsu", "/work/tetsu/target/debug/tetsu", environ(b"DATABASE_URL=postgres://h/tetsu_local")
        )
        self.process(12, "postgres", "/usr/bin/postgres", environ(TEST_URL))
        (self.proc / "self").mkdir()
        with mock.patch("os.kill") as kill:
            self.assertEqual(clean.main(["--test-servers"], proc=self.proc, temporary=self.temporary), 0)
        self.assertEqual([call.args[0] for call in kill.call_args_list], [10])

    def test_a_full_clean_stops_everything_built_here(self):
        root = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(root))
        (root / "mutants.out").mkdir()
        self.process(20, "tetsu", f"{root}/target/debug/tetsu", environ(b"PORT=8080"))
        self.process(21, "tetsu", "/usr/local/bin/tetsu", environ(b"PORT=8080"))
        with mock.patch("os.kill") as kill, mock.patch("subprocess.run") as run:
            run.return_value.returncode = 0
            self.assertEqual(clean.main([], proc=self.proc, root=root, temporary=self.temporary), 0)
        self.assertEqual([call.args[0] for call in kill.call_args_list], [20])
        self.assertFalse((root / "mutants.out").exists())
        self.assertIn("drop_test_databases", run.call_args_list[0].args[0])

    def test_a_process_that_ends_while_read_is_skipped(self):
        self.process(30, "tetsu", "/work/tetsu/target/debug/tetsu", environ(TEST_URL))
        (self.proc / "30" / "environ").unlink()
        self.assertEqual(list(clean.tetsu_processes(self.proc)), [])
        self.assertEqual(list(clean.tetsu_processes(self.proc / "missing")), [])

    def test_a_server_already_gone_is_not_an_error(self):
        with mock.patch("os.kill", side_effect=ProcessLookupError):
            clean.stop([99])

    def test_temporary_files_go_only_once_their_test_process_is_gone(self):
        temporary = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: shutil.rmtree(temporary))
        for name in [
            "tetsu-111-server.pem",
            "t_1794261454_111_6.pem",
            "tetsu-222-env",
            "tetsu-review-notes",
            "x.pem",
        ]:
            (temporary / name).write_text("")
        (temporary / "tetsu-111-folder").mkdir()
        (temporary / "tetsu-111-folder" / ".env").write_text("")
        with mock.patch("os.kill"):
            clean.main(["--test-servers"], proc=self.proc, temporary=temporary, alive=lambda pid: pid == 222)
        self.assertEqual(
            sorted(p.name for p in temporary.iterdir()), ["tetsu-222-env", "tetsu-review-notes", "x.pem"]
        )

    def test_only_a_process_that_exists_is_running(self):
        self.assertTrue(clean.running(os.getpid()))
        self.assertFalse(clean.running(2**30))

    def test_unknown_arguments_print_the_usage(self):
        with mock.patch("sys.stderr"):
            self.assertEqual(clean.main(["--all"], proc=self.proc, temporary=self.temporary), 2)


if __name__ == "__main__":
    unittest.main()
