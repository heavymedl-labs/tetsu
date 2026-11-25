"""The smoke check passes a site that answers as Tetsu does, names each page and header that does not, and
waits for a sleeping instance to wake, but not forever."""

import http.server
import threading
import unittest

from scripts.tests.support import ROOT, run

SCRIPT = ROOT / "scripts" / "smoke_check.py"
GOOD = {
    "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "no-referrer",
    "X-Frame-Options": "DENY",
    "Content-Security-Policy": (
        "default-src 'none'; object-src 'none'; style-src 'self'; frame-ancestors 'none'"
    ),
}


class Site:
    """A local server answering `/` with 200, or each of `pages` with its status, and anything else with 404,
    with these headers, after first answering 503 `waking` times, as a host does while its instance starts."""

    def __init__(self, headers=GOOD, waking=0, root_status=200, pages=None, location=None):
        site = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                if site.waking > 0:
                    site.waking -= 1
                    self.send_response(503)
                    self.end_headers()
                    return
                self.send_response(site.pages.get(self.path, 404))
                for name, value in site.headers.items():
                    self.send_header(name, value)
                if self.path == "/" and site.location:
                    self.send_header("Location", site.location)
                self.end_headers()

            def log_message(self, *_):
                pass

        self.headers, self.waking, self.location = headers, waking, location
        self.pages = {"/": root_status} if pages is None else pages
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()


def smoke(url, *options):
    return run(["python3", SCRIPT, url, "--interval", "0", *options], ROOT)


class SmokeCheckTest(unittest.TestCase):
    def site(self, **settings):
        site = Site(**settings)
        self.addCleanup(site.close)
        return site

    def test_a_site_that_answers_as_tetsu_does_passes(self):
        site = self.site()
        result = smoke(site.url + "/")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("every page answered as expected", result.stdout)

    def test_each_wrong_status_and_header_is_named_with_its_page(self):
        weak = dict(GOOD, **{"X-Frame-Options": "SAMEORIGIN", "Content-Security-Policy": "default-src *"})
        del weak["Referrer-Policy"]
        site = self.site(headers=weak, root_status=500)
        result = smoke(site.url)
        self.assertEqual(result.returncode, 1)
        for problem in [
            "/: status 500, expected 200",
            "/: no referrer-policy header",
            "/: x-frame-options is 'SAMEORIGIN'",
            "/: content-security-policy is 'default-src *'",
            "/Smoke-Check-Missing-Page: no referrer-policy header",
        ]:
            self.assertIn(f"error: {problem}", result.stderr)

    def test_hsts_must_last_a_year(self):
        site = self.site(headers=dict(GOOD, **{"Strict-Transport-Security": "max-age=60"}))
        result = smoke(site.url)
        self.assertIn("error: /: strict-transport-security is 'max-age=60'", result.stderr)

    def test_it_waits_for_a_sleeping_instance_to_wake(self):
        site = self.site(waking=3)
        result = smoke(site.url, "--wait", "60")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_it_gives_up_on_an_instance_that_never_wakes(self):
        site = self.site(waking=10**6)
        result = smoke(site.url, "--wait", "0")
        self.assertEqual(result.returncode, 1)
        self.assertIn("error: /: status 503 after waiting 0 seconds for the site to wake", result.stderr)

    def test_it_gives_up_when_nothing_answers(self):
        site = self.site()
        url = site.url
        site.close()
        result = smoke(url, "--wait", "0")
        self.assertEqual(result.returncode, 1)
        self.assertIn("error: /: no answer after waiting 0 seconds", result.stderr)

    def test_each_host_is_checked_for_its_own_realms_pages(self):
        admin = self.site(pages={"/": 303, "/admin/login": 200}, location="/admin")
        result = smoke(admin.url, "--serves", "admin")
        self.assertEqual(result.returncode, 0, result.stderr)
        result = smoke(admin.url, "--serves", "staff")
        self.assertEqual(result.returncode, 1)
        for problem in ["/app/login: status 404, expected 200", "/admin/login: status 200, expected 404"]:
            self.assertIn(f"error: {problem}", result.stderr)

    def test_a_realms_root_must_send_visitors_to_its_home(self):
        elsewhere = self.site(pages={"/": 303, "/app/login": 200}, location="/admin")
        result = smoke(elsewhere.url, "--serves", "staff")
        self.assertEqual(result.returncode, 1)
        self.assertIn("error: /: sends visitors to '/admin', expected '/app'", result.stderr)

    def test_the_site_serves_no_realms_pages(self):
        site = self.site(pages={"/": 200, "/admin/login": 200})
        result = smoke(site.url)
        self.assertEqual(result.returncode, 1)
        self.assertIn("error: /admin/login: status 200, expected 404", result.stderr)

    def test_bad_arguments_print_the_usage(self):
        for args in [
            [],
            ["tetsu.example.com"],
            ["https://x", "--wait"],
            ["https://x", "--wait", "soon"],
            ["https://x", "--retries", "3"],
            ["https://x", "--serves", "ops"],
            ["https://x", "--serves"],
        ]:
            result = run(["python3", SCRIPT, *args], ROOT)
            self.assertEqual(result.returncode, 2, args)
            self.assertIn("usage: scripts/smoke_check.py", result.stderr)
