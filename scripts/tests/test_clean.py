"""scripts/clean.py: what each mode removes, and what it leaves alone."""

import os
import shutil
import signal
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from scripts import clean

TEST_URL = b"DATABASE_URL=postgres://postgres@localhost:54329/t_1763050000_4242_7"
DAY = 24 * 3600
NOW = 1_800_000_000.0
KEPT = clean.KEPT_COPIES
SETTLE_DAYS = clean.SETTLE // DAY
# One copy more than are kept, newest first, so exactly one is pruned.
HASHES = [f"{index:016x}" for index in range(KEPT + 1)]


def environ(*variables):
    return b"\0".join(variables) + b"\0"


class CleanTest(unittest.TestCase):
    def setUp(self):
        self.proc, self.root, self.temporary = (Path(tempfile.mkdtemp()) for _ in range(3))
        for path in (self.proc, self.root, self.temporary):
            self.addCleanup(shutil.rmtree, path)
        self.host = clean.Host(proc=self.proc, root=self.root, temporary=self.temporary, now=NOW, ci=False)
        self.docker = mock.patch("scripts.clean.docker").start()
        self.run_command = mock.patch("subprocess.run").start()
        self.run_command.return_value.returncode = 0
        self.kill = mock.patch("os.kill").start()
        self.addCleanup(mock.patch.stopall)

    def process(self, pid, name, exe, env):
        place = self.proc / str(pid)
        place.mkdir()
        (place / "comm").write_text(f"{name}\n")
        (place / "environ").write_bytes(env)
        (place / "exe").symlink_to(exe)

    def built(self, path, age_days):
        path = self.root / "target" / path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("")
        os.utime(path, (NOW - age_days * DAY, NOW - age_days * DAY))

    def stopped(self):
        return [call.args[0] for call in self.kill.call_args_list]

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

    def test_routine_stops_test_servers_and_never_one_started_by_hand(self):
        self.process(10, "tetsu", f"{self.root}/target/debug/tetsu", environ(TEST_URL))
        self.process(
            11, "tetsu", f"{self.root}/target/debug/tetsu", environ(b"DATABASE_URL=postgres://h/tetsu_local")
        )
        self.process(12, "postgres", "/usr/bin/postgres", environ(TEST_URL))
        (self.proc / "self").mkdir()
        self.assertEqual(clean.main(["--routine"], self.host), 0)
        self.assertEqual(self.stopped(), [10])
        self.run_command.assert_not_called()

    def test_a_full_clean_stops_everything_built_here_and_drops_test_databases(self):
        (self.root / "mutants.out").mkdir()
        self.process(20, "tetsu", f"{self.root}/target/debug/tetsu", environ(b"PORT=8080"))
        self.process(21, "tetsu", "/usr/local/bin/tetsu", environ(b"PORT=8080"))
        self.assertEqual(clean.main([], self.host), 0)
        self.assertEqual(self.stopped(), [20])
        self.assertFalse((self.root / "mutants.out").exists())
        self.assertIn("drop_test_databases", self.run_command.call_args.args[0])

    def test_routine_prunes_week_old_build_cache_and_a_full_clean_all_of_it(self):
        clean.main(["--routine"], self.host)
        self.assertIn(
            mock.call("builder", "prune", "-f", "--filter", "until=168h"), self.docker.call_args_list
        )
        self.assertIn(mock.call("image", "prune", "-f"), self.docker.call_args_list)
        self.docker.reset_mock()
        clean.main([], self.host)
        self.assertIn(mock.call("builder", "prune", "-f"), self.docker.call_args_list)

    def test_temporary_files_go_only_once_their_test_process_is_gone(self):
        for name in [
            "tetsu-111-server.pem",
            "t_1794261454_111_6.pem",
            "tetsu-222-env",
            "tetsu-review-notes",
            "x.pem",
        ]:
            (self.temporary / name).write_text("")
        (self.temporary / "tetsu-111-folder").mkdir()
        (self.temporary / "tetsu-111-folder" / ".env").write_text("")
        host = clean.Host(
            proc=self.proc, root=self.root, temporary=self.temporary, alive=lambda pid: pid == 222
        )
        clean.main(["--routine"], host)
        left = sorted(path.name for path in self.temporary.iterdir())
        self.assertEqual(left, ["tetsu-222-env", "tetsu-review-notes", "x.pem"])

    def test_superseded_copies_of_a_unit_go_and_its_newest_stay(self):
        for days, hash_ in enumerate(HASHES):
            self.built(f"debug/deps/tetsu-{hash_}", days + SETTLE_DAYS + 1)
            self.built(f"debug/deps/tetsu-{hash_}.d", days + SETTLE_DAYS + 1)
        self.built("debug/deps/libtokio-eeeeeeeeeeeeeeee.rlib", 30)
        self.built("debug/deps/README", 30)
        self.built("nightly/x86_64-unknown-linux-gnu/debug/incremental/tetsu-1111111111111111", 9)
        clean.main(["--routine"], self.host)
        left = sorted(path.name for path in (self.root / "target" / "debug" / "deps").iterdir())
        kept = [name for hash_ in HASHES[:KEPT] for name in (f"tetsu-{hash_}", f"tetsu-{hash_}.d")]
        self.assertEqual(left, sorted(["README", "libtokio-eeeeeeeeeeeeeeee.rlib", *kept]))

    def test_superseded_incremental_folders_go_too(self):
        folders = [f"{index:013d}" for index in range(KEPT + 1)]
        for days, hash_ in enumerate(folders):
            self.built(f"debug/incremental/tetsu-{hash_}/s-session", days + SETTLE_DAYS + 1)
            (self.root / "target" / "debug" / "incremental" / f"tetsu-{hash_}").touch()
            stamp = NOW - (days + SETTLE_DAYS + 1) * DAY
            os.utime(self.root / "target" / "debug" / "incremental" / f"tetsu-{hash_}", (stamp, stamp))
        found = [path.name for path in clean.superseded_builds(self.root / "target", NOW)]
        self.assertEqual(found, [f"tetsu-{folders[KEPT]}"])

    def test_a_copy_younger_than_the_wait_stays_though_superseded(self):
        for hours, hash_ in enumerate(HASHES):
            self.built(f"debug/deps/tetsu-{hash_}", hours / 24)
        self.assertEqual(list(clean.superseded_builds(self.root / "target", NOW)), [])

    def test_copies_deeper_than_a_profile_count_and_mutants_reports_never_do(self):
        for days, hash_ in enumerate(HASHES):
            self.built(f"nightly/x86_64-unknown-linux-gnu/debug/deps/tetsu-{hash_}", days + SETTLE_DAYS + 1)
            self.built(f"mutants.out/deps/tetsu-{hash_}", days + SETTLE_DAYS + 1)
        found = [
            path.relative_to(self.root / "target")
            for path in clean.superseded_builds(self.root / "target", NOW)
        ]
        self.assertEqual(found, [Path(f"nightly/x86_64-unknown-linux-gnu/debug/deps/tetsu-{HASHES[KEPT]}")])
        self.assertEqual(list(clean.superseded_builds(self.root / "missing", NOW)), [])

    def test_ci_prunes_no_build_outputs(self):
        for days, hash_ in enumerate(HASHES):
            self.built(f"debug/deps/tetsu-{hash_}", days + SETTLE_DAYS + 1)
        host = clean.Host(proc=self.proc, root=self.root, temporary=self.temporary, now=NOW, ci=True)
        clean.main(["--routine"], host)
        left = sorted(path.name for path in (self.root / "target" / "debug" / "deps").iterdir())
        self.assertEqual(left, sorted(f"tetsu-{hash_}" for hash_ in HASHES))

    def test_routine_removes_what_a_killed_mutation_or_coverage_run_left(self):
        left = ["target/mutants-tmp/x", ".tools/stryker-tmp/sandbox-1/x", ".tools/coverage-web/.tmp/x"]
        for path in left:
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
            (self.root / path).write_text("")
        (self.root / ".tools" / "bin").mkdir()
        clean.main(["--routine"], self.host)
        self.assertEqual(sorted(p.name for p in (self.root / ".tools").iterdir()), ["bin"])
        self.assertFalse((self.root / "target" / "mutants-tmp").exists())

    def test_routine_kills_a_test_a_killed_mutation_run_left_running(self):
        """A mutant can make a test hang, and a test that caught SIGTERM outlives cargo-mutants' kill, so
        anything still running from a mutation run's copy is killed outright. One in target/debug is not."""
        copy = f"{self.root}/target/mutants-tmp/cargo-mutants-tetsu-x.tmp/target/mutants/deps"
        self.process(40, "tetsu-732bf5d3d", f"{copy}/tetsu-732bf5d3de9c4a48 (deleted)", environ())
        self.process(
            41, "tetsu-0a1b2c3d4", f"{self.root}/target/debug/deps/tetsu-0a1b2c3d4e5f6071", environ()
        )
        self.process(42, "tetsu-0a1b2c3d4", f"{self.root}/target/mutants-tmpx/tetsu", environ())
        clean.main(["--routine"], self.host)
        self.assertEqual(self.kill.call_args_list, [mock.call(40, signal.SIGKILL)])

    def test_a_process_that_ends_while_read_is_skipped(self):
        self.process(30, "tetsu", "/work/tetsu/target/debug/tetsu", environ(TEST_URL))
        (self.proc / "30" / "environ").unlink()
        self.assertEqual(list(clean.tetsu_processes(self.proc)), [])
        self.assertEqual(list(clean.tetsu_processes(self.proc / "missing")), [])
        (self.proc / "31").mkdir()
        self.assertEqual(list(clean.mutation_runs(self.proc, self.root)), [])
        self.assertEqual(list(clean.mutation_runs(self.proc / "missing", self.root)), [])

    def test_a_server_already_gone_is_not_an_error(self):
        self.kill.side_effect = ProcessLookupError
        clean.stop([99])

    def test_only_a_process_that_exists_is_running(self):
        self.assertTrue(clean.running(os.getpid()))
        self.assertFalse(clean.running(2**30))

    def test_docker_runs_only_where_it_is_installed(self):
        mock.patch.stopall()
        with mock.patch("shutil.which", return_value=None), mock.patch("subprocess.run") as run:
            clean.docker("image", "prune", "-f")
        run.assert_not_called()
        with mock.patch("shutil.which", return_value="/usr/bin/docker"), mock.patch("subprocess.run") as run:
            clean.docker("image", "prune", "-f")
        self.assertEqual(run.call_args.args[0], ["docker", "image", "prune", "-f"])

    def test_unknown_arguments_print_the_usage(self):
        with mock.patch("sys.stderr"):
            self.assertEqual(clean.main(["--all"], self.host), 2)


if __name__ == "__main__":
    unittest.main()
