"""The local Mailpit makes its certificate and login once, and prints the settings that reach it."""

import json
import shutil
import stat
import tempfile
import unittest
from pathlib import Path

from scripts.tests.support import ROOT, run, write

# Each logs how it was run. openssl writes the files it was asked for; mailpit exits as MAILPIT_EXIT says.
OPENSSL = """#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
open(os.environ["LOG"], "a").write(json.dumps(["openssl", *args]) + "\\n")
for flag in ("-keyout", "-out"):
    open(args[args.index(flag) + 1], "w").write(flag)
"""
MAILPIT = """#!/usr/bin/env python3
import json, os, sys
open(os.environ["LOG"], "a").write(json.dumps(["mailpit", *sys.argv[1:]]) + "\\n")
sys.exit(int(os.environ.get("MAILPIT_EXIT") or 0))
"""


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

    def start(self, *addresses, exit_code=0):
        return run(
            ["python3", self.root / "scripts" / "mailpit.py", *addresses],
            self.root,
            PATH=f"{self.root / 'bin'}:/usr/bin:/bin",
            LOG=str(self.log),
            MAILPIT_EXIT=str(exit_code),
        )

    def calls(self, program):
        lines = self.log.read_text().splitlines() if self.log.exists() else []
        return [call[1:] for call in map(json.loads, lines) if call[0] == program]

    def settings(self, output):
        return dict(line.split("=", 1) for line in output.splitlines() if line.startswith("TETSU_"))

    def test_it_makes_a_certificate_and_login_and_prints_the_settings_that_reach_it(self):
        result = self.start()
        self.assertEqual(result.returncode, 0, result.stderr)
        (made,) = self.calls("openssl")
        self.assertIn("subjectAltName=DNS:localhost", made)
        self.assertIn("basicConstraints=critical,CA:FALSE", made)
        settings = self.settings(result.stdout)
        self.assertEqual(settings["TETSU_SMTP_HOST"], "localhost")
        self.assertEqual(settings["TETSU_SMTP_PORT"], "1465")
        self.assertEqual(settings["TETSU_SMTP_USERNAME"], "tetsu")
        self.assertRegex(settings["TETSU_SMTP_PASSWORD"], r"^[0-9a-f]{32}$")
        self.assertEqual(settings["TETSU_SMTP_CA_FILE"], str(self.files / "cert.pem"))
        self.assertEqual(settings["TETSU_MAIL_FROM"], "tetsu@tetsu.localhost")
        self.assertEqual((self.files / "auth").read_text(), f"tetsu:{settings['TETSU_SMTP_PASSWORD']}\n")
        self.assertEqual(stat.S_IMODE((self.files / "auth").stat().st_mode), 0o600)
        self.assertIn("http://localhost:8025", result.stdout)

    def test_mailpit_answers_only_on_this_machine_and_only_over_tls_with_the_login(self):
        self.start()
        (args,) = self.calls("mailpit")
        given = dict(zip(args, args[1:]))
        self.assertEqual(given["--smtp"], "127.0.0.1:1465")
        self.assertEqual(given["--listen"], "127.0.0.1:8025")
        self.assertEqual(given["--smtp-tls-cert"], str(self.files / "cert.pem"))
        self.assertEqual(given["--smtp-tls-key"], str(self.files / "key.pem"))
        self.assertEqual(given["--smtp-auth-file"], str(self.files / "auth"))
        self.assertEqual(given["--database"], str(self.files / "mailpit.db"))
        self.assertIn("--smtp-require-tls", args)

    def test_a_second_start_keeps_the_certificate_and_login(self):
        first = self.settings(self.start().stdout)
        second = self.settings(self.start().stdout)
        self.assertEqual(len(self.calls("openssl")), 1)
        self.assertEqual(first, second)

    def test_a_missing_certificate_is_made_again(self):
        self.start()
        (self.files / "cert.pem").unlink()
        self.start()
        self.assertEqual(len(self.calls("openssl")), 2)

    def test_addresses_given_are_the_ones_that_reach_mailpit(self):
        result = self.start("avery@example.com", "lee@example.org")
        self.assertEqual(
            self.settings(result.stdout)["TETSU_MAIL_ALLOW"], "avery@example.com,lee@example.org"
        )

    def test_without_addresses_it_says_how_to_add_them(self):
        result = self.start()
        self.assertNotIn("TETSU_MAIL_ALLOW", self.settings(result.stdout))
        self.assertIn("scripts/mailpit.py you@example.com", result.stdout)

    def test_mailpits_exit_status_is_the_scripts(self):
        self.assertEqual(self.start(exit_code=3).returncode, 3)

    def test_without_mailpit_installed_it_says_how_to_install_it(self):
        (self.root / ".tools" / "bin" / "mailpit").unlink()
        result = self.start()
        self.assertEqual(result.returncode, 1)
        self.assertIn("scripts/install-tools.sh", result.stderr)
        self.assertEqual(self.calls("openssl"), [])


if __name__ == "__main__":
    unittest.main()
