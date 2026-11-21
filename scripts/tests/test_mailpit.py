"""The local Mailpit makes its certificate and login once, and gives Tetsu a settings file while it runs."""

import json
import os
import re
import shutil
import signal
import stat
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, clean_env, write

# Each logs how it was run. openssl writes the files it was asked for. mailpit also logs .env.mailpit's mode
# and text as Mailpit starts, then waits if MAILPIT_WAIT names a file for its process id, and exits as
# MAILPIT_EXIT says.
OPENSSL = """#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
open(os.environ["LOG"], "a").write(json.dumps(["openssl", *args]) + "\\n")
for flag in ("-keyout", "-out"):
    open(args[args.index(flag) + 1], "w").write(flag)
# As a second run would, starting at the same moment: its file appears after this run's check.
if os.environ.get("RACING"):
    open(os.environ["SETTINGS"], "w").write("TETSU_SMTP_HOST=other\\n")
"""
MAILPIT = """#!/usr/bin/env python3
import json, os, sys, time
mode = oct(os.stat(os.environ["SETTINGS"]).st_mode & 0o777)
settings = open(os.environ["SETTINGS"]).read()
open(os.environ["LOG"], "a").write(json.dumps(["mailpit", mode, settings, *sys.argv[1:]]) + "\\n")
if os.environ.get("MAILPIT_WAIT"):
    open(os.environ["MAILPIT_WAIT"], "w").write(str(os.getpid()))
    time.sleep(60)
sys.exit(int(os.environ.get("MAILPIT_EXIT") or 0))
"""
LOCAL = "DATABASE_URL=postgres://postgres@localhost:54329/tetsu_local\nTETSU_DEMO=true\n"
# What a person's shell may export, which would change what the script does, so the tests never inherit it.
INHERITED = re.compile(r"DATABASE_URL|TETSU_(SMTP|MAIL)_\w+")


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
        self.settings = self.root / ".env.mailpit"

    def environment(self, **extra):
        inherited = {name: value for name, value in clean_env().items() if not INHERITED.fullmatch(name)}
        return {
            **inherited,
            "PATH": f"{self.root / 'bin'}:/usr/bin:/bin",
            "LOG": str(self.log),
            "SETTINGS": str(self.settings),
            **extra,
        }

    def start(self, *addresses, exit_code=0, **extra):
        return subprocess.run(
            ["python3", str(self.root / "scripts" / "mailpit.py"), *addresses],
            cwd=self.root,
            env=self.environment(MAILPIT_EXIT=str(exit_code), **extra),
            capture_output=True,
            text=True,
            stdin=subprocess.DEVNULL,
            check=False,
        )

    def stopped_by(self, sent):
        """Starts the script with a Mailpit that waits, sends the script `sent`, and returns its exit status
        and the waiting Mailpit's process id once both have ended."""
        mailpit = self.root / "mailpit.pid"
        mailpit.unlink(missing_ok=True)
        script = subprocess.Popen(
            ["python3", str(self.root / "scripts" / "mailpit.py"), "avery@example.com"],
            cwd=self.root,
            env=self.environment(MAILPIT_WAIT=str(mailpit)),
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
        """The settings file's mode and text as Mailpit started, and its arguments, from its last start."""
        mode, text, *args = self.calls("mailpit")[-1]
        return mode, text, args

    def setting(self, text):
        return dict(line.split("=", 1) for line in text.splitlines() if line and not line.startswith("#"))

    def test_while_it_runs_tetsu_has_a_settings_file_of_its_own_that_reaches_mailpit(self):
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 0, result.stderr)
        (made,) = self.calls("openssl")
        self.assertIn("subjectAltName=DNS:localhost", made)
        self.assertIn("basicConstraints=critical,CA:FALSE", made)
        _, text, _ = self.running()
        settings = self.setting(text)
        self.assertEqual(settings["DATABASE_URL"], "postgres://postgres@localhost:54329/tetsu_local")
        self.assertEqual(settings["TETSU_SMTP_HOST"], "localhost")
        self.assertEqual(settings["TETSU_SMTP_PORT"], "1465")
        self.assertEqual(settings["TETSU_SMTP_USERNAME"], "tetsu")
        self.assertRegex(settings["TETSU_SMTP_PASSWORD"], r"^[0-9a-f]{32}$")
        self.assertEqual(settings["TETSU_SMTP_CA_FILE"], str(self.files / "cert.pem"))
        self.assertEqual(settings["TETSU_MAIL_FROM"], "tetsu@tetsu.localhost")
        self.assertEqual(settings["TETSU_MAIL_ALLOW"], "avery@example.com")
        self.assertEqual((self.files / "auth").read_text(), f"tetsu:{settings['TETSU_SMTP_PASSWORD']}\n")
        self.assertEqual(stat.S_IMODE((self.files / "auth").stat().st_mode), 0o600)
        # The password stays in the file, never on the screen.
        self.assertNotIn(settings["TETSU_SMTP_PASSWORD"], result.stdout)

    def test_env_is_never_touched(self):
        self.start("avery@example.com")
        self.assertEqual(self.env.read_text(), LOCAL)

    def test_its_file_is_readable_only_by_its_owner_from_the_start(self):
        # Mailpit sees the file as it starts, the first moment anything else could read it.
        self.start("avery@example.com")
        mode, _, _ = self.running()
        self.assertEqual(mode, "0o600")

    def test_mail_settings_of_your_own_in_env_are_left_out_of_its_file(self):
        self.env.write_text(LOCAL + "TETSU_SMTP_HOST=smtp.mailgun.org\nTETSU_MAIL_ALLOW=lee@example.org\n")
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 0, result.stderr)
        _, text, _ = self.running()
        self.assertEqual(text.count("TETSU_SMTP_HOST="), 1)
        self.assertEqual(self.setting(text)["TETSU_MAIL_ALLOW"], "avery@example.com")

    def test_its_file_goes_once_mailpit_stops(self):
        for exit_code in (0, 3):
            with self.subTest(exit_code=exit_code):
                self.assertEqual(self.start("avery@example.com", exit_code=exit_code).returncode, exit_code)
                self.assertFalse(self.settings.exists())

    def test_any_way_it_is_stopped_removes_its_file_and_stops_mailpit(self):
        # Ctrl+C, Ctrl+\\, a closed terminal, or kill.
        for sent in (signal.SIGINT, signal.SIGQUIT, signal.SIGHUP, signal.SIGTERM):
            with self.subTest(sent=sent.name):
                status, mailpit = self.stopped_by(sent)
                self.assertEqual(status, 0)
                self.assertFalse(self.settings.exists())
                with self.assertRaises(ProcessLookupError):
                    os.kill(mailpit, 0)

    def test_a_file_already_there_is_left_alone(self):
        # Another run's, or one kill -9 left: either way it is the person's to remove.
        self.settings.write_text("TETSU_SMTP_HOST=localhost\n")
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 1)
        self.assertIn(".env.mailpit already exists", result.stderr)
        self.assertEqual(self.settings.read_text(), "TETSU_SMTP_HOST=localhost\n")
        self.assertEqual(self.calls("mailpit"), [])

    def test_a_file_that_appears_after_the_check_is_never_overwritten_or_removed(self):
        result = self.start("avery@example.com", RACING="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.settings.read_text(), "TETSU_SMTP_HOST=other\n")
        self.assertEqual(self.calls("mailpit"), [])

    def test_mail_settings_in_the_environment_are_refused(self):
        # Even an empty one, which Tetsu still prefers to its settings file's.
        result = self.start("avery@example.com", TETSU_SMTP_HOST="smtp.mailgun.org", TETSU_MAIL_ALLOW="")
        self.assertEqual(result.returncode, 1)
        self.assertIn("the environment sets TETSU_MAIL_ALLOW, TETSU_SMTP_HOST", result.stderr)
        self.assertFalse(self.settings.exists())

    def test_an_exported_database_url_is_refused(self):
        # Tetsu then takes no mail setting from a file, so the file would do nothing.
        result = self.start("avery@example.com", DATABASE_URL="postgres://elsewhere/db")
        self.assertEqual(result.returncode, 1)
        self.assertIn("the environment sets DATABASE_URL", result.stderr)
        self.assertFalse(self.settings.exists())

    def test_mailpit_answers_only_on_this_machine_and_only_over_tls_with_the_login(self):
        self.start("avery@example.com")
        _, _, args = self.running()
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
        first = self.setting(self.running()[1])
        self.start("avery@example.com")
        second = self.setting(self.running()[1])
        self.assertEqual(len(self.calls("openssl")), 1)
        self.assertEqual(first, second)

    def test_a_missing_certificate_is_made_again(self):
        self.start("avery@example.com")
        (self.files / "cert.pem").unlink()
        self.start("avery@example.com")
        self.assertEqual(len(self.calls("openssl")), 2)

    def test_several_addresses_all_reach_mailpit(self):
        self.start("avery@example.com", "lee@example.org")
        allowed = self.setting(self.running()[1])["TETSU_MAIL_ALLOW"]
        self.assertEqual(allowed, "avery@example.com,lee@example.org")

    def test_it_prints_the_next_steps_with_the_address_and_its_settings_file(self):
        result = self.start("avery@example.com")
        self.assertIn("http://localhost:8025", result.stdout)
        self.assertIn(
            'cargo tetsu --env mailpit operator create "Your Name" avery@example.com', result.stdout
        )
        self.assertIn("cargo tetsu --env mailpit operator email <id> avery@example.com", result.stdout)
        self.assertIn("cargo tetsu --env mailpit serve", result.stdout)

    def test_an_address_is_required(self):
        result = self.start()
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: scripts/mailpit.py", result.stderr)
        self.assertEqual(self.calls("mailpit"), [])

    def test_without_env_it_says_to_make_one(self):
        self.env.unlink()
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 1)
        self.assertIn("cp .env.example .env", result.stderr)
        self.assertFalse(self.settings.exists())

    def test_without_mailpit_installed_it_says_how_to_install_it(self):
        (self.root / ".tools" / "bin" / "mailpit").unlink()
        result = self.start("avery@example.com")
        self.assertEqual(result.returncode, 1)
        self.assertIn("scripts/install-tools.sh", result.stderr)
        self.assertEqual(self.calls("openssl"), [])
        self.assertFalse(self.settings.exists())


if __name__ == "__main__":
    unittest.main()
