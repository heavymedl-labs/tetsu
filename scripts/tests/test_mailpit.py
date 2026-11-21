"""The local Mailpit makes its certificate and login once, and points .env at itself only while it runs."""

import json
import os
import shutil
import signal
import stat
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, clean_env, run, write

# Each logs how it was run. openssl writes the files it was asked for. mailpit also logs .env as it stands
# while Mailpit runs, then exits as MAILPIT_EXIT says.
OPENSSL = """#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
open(os.environ["LOG"], "a").write(json.dumps(["openssl", *args]) + "\\n")
for flag in ("-keyout", "-out"):
    open(args[args.index(flag) + 1], "w").write(flag)
"""
MAILPIT = """#!/usr/bin/env python3
import json, os, sys, time
env = open(os.environ["ENV_FILE"]).read()
open(os.environ["LOG"], "a").write(json.dumps(["mailpit", env, *sys.argv[1:]]) + "\\n")
if os.environ.get("MAILPIT_APPEND"):
    open(os.environ["ENV_FILE"], "a").write(os.environ["MAILPIT_APPEND"])
if os.environ.get("MAILPIT_WAIT"):
    open(os.environ["MAILPIT_WAIT"], "w").write(str(os.getpid()))
    time.sleep(60)
sys.exit(int(os.environ.get("MAILPIT_EXIT") or 0))
"""
LOCAL = "DATABASE_URL=postgres://postgres@localhost:54329/tetsu_local\nTETSU_DEMO=true\n"


class Mailpit(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts" / "mailpit.py", self.root / "scripts")
        write(self.root / "bin" / "openssl", OPENSSL, stat.S_IRWXU)
        write(self.root / ".tools" / "bin" / "mailpit", MAILPIT, stat.S_IRWXU)
        self.log = self.root / "log"
        self.files = self.root / ".tools" / "mailpit"
        self.env = self.root / ".env"
        self.env.write_text(LOCAL)

    def environment(self, **extra):
        return {
            "PATH": f"{self.root / 'bin'}:/usr/bin:/bin",
            "LOG": str(self.log),
            "ENV_FILE": str(self.env),
            **extra,
        }

    def start(self, *addresses, exit_code=0, **extra):
        return run(
            ["python3", self.root / "scripts" / "mailpit.py", *addresses],
            self.root,
            **self.environment(MAILPIT_EXIT=str(exit_code), **extra),
        )

    def stopped_by(self, sent):
        """Starts the script with a Mailpit that waits, sends the script `sent`, and returns its exit status
        and the waiting Mailpit's process id once both have ended."""
        mailpit = self.root / "mailpit.pid"
        mailpit.unlink(missing_ok=True)
        script = subprocess.Popen(
            ["python3", str(self.root / "scripts" / "mailpit.py"), "avery@example.com"],
            cwd=self.root,
            env=clean_env(**self.environment(MAILPIT_WAIT=str(mailpit))),
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        deadline = time.monotonic() + 10
        while not mailpit.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        script.send_signal(sent)
        status = script.wait(timeout=10)
        return status, int(mailpit.read_text())

    def calls(self, program):
        lines = self.log.read_text().splitlines() if self.log.exists() else []
        return [call[1:] for call in map(json.loads, lines) if call[0] == program]

    def running(self):
        """The settings .env held while Mailpit ran, and the arguments it ran with, from its last start."""
        env, *args = self.calls("mailpit")[-1]
        settings = dict(line.split("=", 1) for line in env.splitlines() if line.startswith("TETSU_"))
        return settings, args

    def test_it_points_env_at_mailpit_while_it_runs(self):
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 0, result.stderr)
        (made,) = self.calls("openssl")
        self.assertIn("subjectAltName=DNS:localhost", made)
        self.assertIn("basicConstraints=critical,CA:FALSE", made)
        settings, _ = self.running()
        self.assertEqual(settings["TETSU_SMTP_HOST"], "localhost")
        self.assertEqual(settings["TETSU_SMTP_PORT"], "1465")
        self.assertEqual(settings["TETSU_SMTP_USERNAME"], "tetsu")
        self.assertRegex(settings["TETSU_SMTP_PASSWORD"], r"^[0-9a-f]{32}$")
        self.assertEqual(settings["TETSU_SMTP_CA_FILE"], str(self.files / "cert.pem"))
        self.assertEqual(settings["TETSU_MAIL_FROM"], "tetsu@tetsu.localhost")
        self.assertEqual(settings["TETSU_MAIL_ALLOW"], "avery@example.com")
        self.assertEqual((self.files / "auth").read_text(), f"tetsu:{settings['TETSU_SMTP_PASSWORD']}\n")
        self.assertEqual(stat.S_IMODE((self.files / "auth").stat().st_mode), 0o600)
        # The password stays in .env, never on the screen.
        self.assertNotIn(settings["TETSU_SMTP_PASSWORD"], result.stdout)

    def test_env_is_as_it_was_once_mailpit_stops(self):
        for exit_code in (0, 3):
            with self.subTest(exit_code=exit_code):
                self.assertEqual(self.start("avery@example.com", exit_code=exit_code).returncode, exit_code)
                self.assertEqual(self.env.read_text(), LOCAL)

    def test_an_edit_made_while_it_runs_is_kept(self):
        self.start("avery@example.com", MAILPIT_APPEND="TETSU_LOG_FORMAT=json\n")
        self.assertEqual(self.env.read_text(), LOCAL + "TETSU_LOG_FORMAT=json\n")

    def test_any_way_it_is_stopped_takes_its_settings_out_and_stops_mailpit(self):
        # Ctrl+C, a closed terminal, or kill.
        for sent in (signal.SIGINT, signal.SIGHUP, signal.SIGTERM):
            with self.subTest(sent=sent.name):
                status, mailpit = self.stopped_by(sent)
                self.assertEqual(status, 0)
                self.assertEqual(self.env.read_text(), LOCAL)
                with self.assertRaises(ProcessLookupError):
                    os.kill(mailpit, 0)

    def test_env_keeps_its_permissions(self):
        self.env.chmod(0o600)
        self.start("avery@example.com")
        self.assertEqual(stat.S_IMODE(self.env.stat().st_mode), 0o600)

    def test_a_running_mailpits_settings_are_left_alone(self):
        live = (
            LOCAL
            + f"\n# Local Mailpit for process {os.getpid()}, set by scripts/mailpit.py until it stops.\n"
        )
        live += "TETSU_SMTP_HOST=localhost\n# End of local Mailpit.\n"
        self.env.write_text(live)
        result = self.start("lee@example.org")
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"a local Mailpit already runs, as process {os.getpid()}", result.stderr)
        self.assertEqual(self.env.read_text(), live)
        self.assertEqual(self.calls("mailpit"), [])

    def test_mail_settings_in_the_environment_are_never_overridden(self):
        result = self.start("avery@example.com", TETSU_SMTP_HOST="smtp.mailgun.org", TETSU_MAIL_FROM="")
        self.assertEqual(result.returncode, 1)
        self.assertIn("the environment sets TETSU_SMTP_HOST", result.stderr)
        self.assertNotIn("TETSU_MAIL_FROM", result.stderr)
        self.assertEqual(self.env.read_text(), LOCAL)

    def test_the_rest_of_env_is_kept_while_it_runs(self):
        self.start("avery@example.com")
        env, *_ = self.calls("mailpit")[-1]
        self.assertTrue(env.startswith(LOCAL))

    def test_a_block_a_killed_run_left_is_replaced_not_repeated(self):
        self.start("avery@example.com")
        env, *_ = self.calls("mailpit")[-1]
        self.env.write_text(env)
        self.start("lee@example.org")
        settings, _ = self.running()
        again, *_ = self.calls("mailpit")[-1]
        self.assertEqual(again.count("TETSU_SMTP_HOST="), 1)
        self.assertEqual(settings["TETSU_MAIL_ALLOW"], "lee@example.org")
        self.assertEqual(self.env.read_text(), LOCAL)

    def test_mail_settings_of_your_own_are_never_overridden(self):
        own = LOCAL + "TETSU_SMTP_HOST=smtp.mailgun.org\n# TETSU_MAIL_FROM=commented@example.com\n"
        self.env.write_text(own)
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 1)
        self.assertIn(".env already sets TETSU_SMTP_HOST", result.stderr)
        self.assertNotIn("TETSU_MAIL_FROM", result.stderr)
        self.assertEqual(self.env.read_text(), own)
        self.assertEqual(self.calls("mailpit"), [])

    def test_without_env_it_says_to_make_one(self):
        self.env.unlink()
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 1)
        self.assertIn("cp .env.example .env", result.stderr)
        self.assertFalse(self.env.exists())

    def test_mailpit_answers_only_on_this_machine_and_only_over_tls_with_the_login(self):
        self.start("avery@example.com")
        _, args = self.running()
        given = dict(zip(args, args[1:]))
        self.assertEqual(given["--smtp"], "127.0.0.1:1465")
        self.assertEqual(given["--listen"], "127.0.0.1:8025")
        self.assertEqual(given["--smtp-tls-cert"], str(self.files / "cert.pem"))
        self.assertEqual(given["--smtp-tls-key"], str(self.files / "key.pem"))
        self.assertEqual(given["--smtp-auth-file"], str(self.files / "auth"))
        self.assertEqual(given["--database"], str(self.files / "mailpit.db"))
        self.assertIn("--smtp-require-tls", args)

    def test_a_second_start_keeps_the_certificate_and_login(self):
        self.start("avery@example.com")
        first, _ = self.running()
        self.start("avery@example.com")
        second, _ = self.running()
        self.assertEqual(len(self.calls("openssl")), 1)
        self.assertEqual(first, second)

    def test_a_missing_certificate_is_made_again(self):
        self.start("avery@example.com")
        (self.files / "cert.pem").unlink()
        self.start("avery@example.com")
        self.assertEqual(len(self.calls("openssl")), 2)

    def test_several_addresses_all_reach_mailpit(self):
        self.start("avery@example.com", "lee@example.org")
        settings, _ = self.running()
        self.assertEqual(settings["TETSU_MAIL_ALLOW"], "avery@example.com,lee@example.org")

    def test_it_prints_the_next_steps_with_the_address(self):
        result = self.start("avery@example.com")
        self.assertIn("http://localhost:8025", result.stdout)
        self.assertIn('cargo tetsu operator create "Your Name" avery@example.com', result.stdout)
        self.assertIn("cargo tetsu operator email <id> avery@example.com", result.stdout)
        self.assertIn("cargo tetsu serve", result.stdout)

    def test_an_address_is_required(self):
        result = self.start()
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: scripts/mailpit.py", result.stderr)
        self.assertEqual(self.calls("mailpit"), [])

    def test_without_mailpit_installed_it_says_how_to_install_it(self):
        (self.root / ".tools" / "bin" / "mailpit").unlink()
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 1)
        self.assertIn("scripts/install-tools.sh", result.stderr)
        self.assertEqual(self.calls("openssl"), [])
        self.assertEqual(self.env.read_text(), LOCAL)


if __name__ == "__main__":
    unittest.main()
