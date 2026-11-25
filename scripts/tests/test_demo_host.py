"""The host script deploys one image to every service or to none it could not check first, reads a
service's database URL, and installs the host's command line tool only when its checksum matches."""

import hashlib
import http.server
import io
import json
import tempfile
import threading
import unittest
import zipfile
from pathlib import Path

from scripts.tests.support import ROOT, run, write

SCRIPT = ROOT / "scripts" / "host.py"
DIGEST = "ghcr.io/heavymedl-labs/tetsu@sha256:" + "a" * 64
EXECUTABLE = 0o755
# What the script is given as the host's token in these tests.
STAND_IN = "stand-in"


class Server:
    """A local server answering each path in `answers` with its status and body, and anything else with 404,
    recording each request's path and authorization."""

    def __init__(self, answers):
        server = self
        self.seen = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                server.seen.append((self.path, self.headers.get("Authorization")))
                status, body = answers.get(self.path, (404, b""))
                self.send_response(status)
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_):
                pass

        self.http = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.http.server_address[1]}"
        threading.Thread(target=self.http.serve_forever, daemon=True).start()

    def close(self):
        self.http.shutdown()
        self.http.server_close()


class HostTest(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.root = Path(folder.name)
        self.bin = self.root / "bin"
        self.log = self.root / "log"

    def host(self, *args, cli="exit 0", **env):
        """Runs the script with a stand-in for the host's tool that logs each call and then runs `cli`."""
        write(
            self.bin / "render", f'#!/bin/sh\necho "$RENDER_API_KEY $*" >> "{self.log}"\n{cli}\n', EXECUTABLE
        )
        return run(
            ["python3", SCRIPT, *args],
            self.root,
            PATH=f"{self.bin}:/usr/bin:/bin",
            **{"HOST_TOKEN": STAND_IN},
            **env,
        )

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_one_digest_reaches_every_service(self):
        result = self.host("deploy", DIGEST, "srv-site", "srv-admin", "srv-app")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            self.calls(),
            [
                f"{STAND_IN} deploys create {service} --image {DIGEST} --wait --confirm -o text"
                for service in ("srv-site", "srv-admin", "srv-app")
            ],
        )

    def test_a_missing_service_deploys_nothing(self):
        result = self.host("deploy", DIGEST, "srv-site", "", "srv-app")
        self.assertEqual(result.returncode, 1)
        self.assertIn("every realm's service must be named", result.stderr)
        self.assertEqual(self.calls(), [])

    def test_one_service_named_twice_deploys_nothing(self):
        result = self.host("deploy", DIGEST, "srv-site", "srv-app", "srv-app")
        self.assertEqual(result.returncode, 1)
        self.assertIn("each realm's service must be its own", result.stderr)
        self.assertEqual(self.calls(), [])

    def services(self):
        """A stand-in for the host's API, holding a service for each realm at its own address."""
        api = Server(
            {
                f"/services/srv-{name}": (
                    200,
                    json.dumps({"serviceDetails": {"url": f"https://{name}.example"}}).encode(),
                )
                for name in ("site", "admin", "app")
            }
        )
        self.addCleanup(api.close)
        return api

    def test_a_check_confirms_each_service_with_the_host_before_anything_changes(self):
        api = self.services()
        pairs = [
            "srv-site=https://site.example",
            "srv-admin=https://admin.example",
            "srv-app=https://app.example",
        ]
        result = self.host("check", *pairs, HOST_API=api.url)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            [path for path, _ in api.seen], ["/services/srv-site", "/services/srv-admin", "/services/srv-app"]
        )
        self.assertTrue(all(authorization == f"Bearer {STAND_IN}" for _, authorization in api.seen))
        self.assertEqual(self.calls(), [])

    def test_a_check_refuses_what_would_stop_or_misdirect_a_deploy(self):
        api = self.services()
        for pairs, said in [
            (
                ["srv-site=https://site.example", "=https://admin.example"],
                "every realm's service must be named",
            ),
            (
                ["srv-site=https://site.example", "srv-site=https://site.example"],
                "each realm's service must be its own",
            ),
            (["srv-site="], "every realm's origin must be set"),
            (["srv-site"], "every realm's origin must be set"),
            (["srv-gone=https://gone.example"], "srv-gone could not be read from the host"),
            (
                ["srv-admin=https://site.example"],
                "srv-admin serves https://admin.example, not https://site.example",
            ),
        ]:
            result = self.host("check", *pairs, HOST_API=api.url)
            self.assertEqual(result.returncode, 1, pairs)
            self.assertIn(said, result.stderr)
        unset = run(
            ["python3", SCRIPT, "check", "srv-site=https://site.example"], self.root, PATH="/usr/bin:/bin"
        )
        self.assertEqual(unset.returncode, 1)
        self.assertIn("HOST_TOKEN must be set", unset.stderr)

    def test_a_failed_deploy_names_what_was_deployed(self):
        result = self.host("deploy", DIGEST, "srv-site", "srv-admin", cli='[ "$3" != srv-admin ]')
        self.assertEqual(result.returncode, 1)
        self.assertIn("srv-admin did not deploy; srv-site runs the new image", result.stderr)

    def test_a_service_database_url_is_read_with_the_token(self):
        api = Server(
            {"/services/srv-site/env-vars/DATABASE_URL": (200, b'{"value": "postgres://app@db/tetsu"}')}
        )
        self.addCleanup(api.close)
        result = self.host("database-url", "srv-site", HOST_API=api.url)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "postgres://app@db/tetsu\n")
        self.assertEqual(api.seen, [("/services/srv-site/env-vars/DATABASE_URL", f"Bearer {STAND_IN}")])

    def test_a_database_url_that_is_not_postgres_is_refused(self):
        api = Server(
            {"/services/srv-site/env-vars/DATABASE_URL": (200, json.dumps({"value": "nope"}).encode())}
        )
        self.addCleanup(api.close)
        result = self.host("database-url", "srv-site", HOST_API=api.url)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("could not be read as a Postgres URL", result.stderr)
        missing = self.host("database-url", "srv-gone", HOST_API=api.url)
        self.assertEqual(missing.returncode, 1)
        self.assertIn("could not be read as a Postgres URL", missing.stderr)

    def tool(self):
        """A zip holding the host's tool as its release names it, and that zip's SHA-256."""
        data = io.BytesIO()
        with zipfile.ZipFile(data, "w") as archive:
            archive.writestr("cli_v9.9.9", "#!/bin/sh\necho tool\n")
        return data.getvalue(), hashlib.sha256(data.getvalue()).hexdigest()

    def test_the_tool_is_installed_only_when_its_checksum_matches(self):
        zipped, checksum = self.tool()
        releases = Server({"/v9.9.9/cli_9.9.9_linux_amd64.zip": (200, zipped)})
        self.addCleanup(releases.close)
        settings = {"HOST_CLI_URL": releases.url, "HOST_CLI_VERSION": "9.9.9"}
        target = self.root / "tools"
        wrong = self.host("install", target, HOST_CLI_SHA256="0" * 64, **settings)
        self.assertEqual(wrong.returncode, 1)
        self.assertIn("checksum", wrong.stderr)
        self.assertFalse((target / "render").exists())
        right = self.host("install", target, HOST_CLI_SHA256=checksum, **settings)
        self.assertEqual(right.returncode, 0, right.stderr)
        self.assertTrue((target / "render").stat().st_mode & 0o100)

    def test_bad_arguments_print_the_usage(self):
        for args in [
            [],
            ["deploy"],
            ["deploy", "not-a-digest", "srv"],
            ["check"],
            ["database-url"],
            ["install"],
            ["ship"],
        ]:
            result = self.host(*args)
            self.assertEqual(result.returncode, 2, args)
            self.assertIn("usage: scripts/host.py", result.stderr)


if __name__ == "__main__":
    unittest.main()
