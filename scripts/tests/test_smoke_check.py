"""The smoke check passes a site that answers as Tetsu does, names each page and header that does not, and
waits for a sleeping instance to wake, but not forever."""

import http.server
import threading
import unittest

from scripts.tests.support import ROOT, run

SCRIPT = ROOT / "scripts" / "smoke_check.py"
# The staff login a site's `/login` leads to.
LOGIN = "https://app.example.com/login"
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
    """A local server answering `/` with 200 and `/login` with 303, or each of `pages` with its status, and
    anything else with 404, with these headers, after first answering 503 `waking` times, as a host does
    while its instance starts."""

    def __init__(self, headers=GOOD, waking=0, root_status=200, pages=None, locations=None):
        site = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                if site.waking > 0:
                    site.waking -= 1
                    self.send_response(503)
                    self.end_headers()
                    return
                # A page given as a pair also names the referrer policy it sends, as a realm's own pages do.
                status, referrer = (site.pages.get(self.path, 404), None)
                if isinstance(status, tuple):
                    status, referrer = status
                self.send_response(status)
                for name, value in site.headers.items():
                    self.send_header(name, referrer if referrer and name == "Referrer-Policy" else value)
                if self.path in site.locations:
                    self.send_header("Location", site.locations[self.path])
                self.end_headers()

            def log_message(self, *_):
                pass

        self.headers, self.waking = headers, waking
        self.locations = {"/login": LOGIN} if locations is None else locations
        self.pages = {"/": root_status, "/login": 303} if pages is None else pages
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_address[1]}"
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()


def smoke(url, *options):
    """Runs the check on `url`, naming the staff login a site leads to unless `options` say what it serves."""
    login = [] if "--serves" in options else ["--login", LOGIN]
    return run(["python3", SCRIPT, url, "--interval", "0", *login, *options], ROOT)


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

    def realm(self, **settings):
        """A realm's host as Tetsu serves it: its root and any page it does not hold send a visitor to log in,
        and its login sends a referrer to Tetsu alone."""
        pages = {"/": 303, "/login": (200, "same-origin"), "/Smoke-Check-Missing-Page": 303}
        return self.site(**dict({"pages": pages, "locations": {"/": "/login"}}, **settings))

    def test_each_realms_host_serves_its_pages_at_its_root(self):
        admin = self.realm()
        for serves in ["admin", "staff"]:
            result = smoke(admin.url, "--serves", serves)
            self.assertEqual(result.returncode, 0, result.stderr)
        result = smoke(admin.url)
        self.assertEqual(result.returncode, 1)
        for problem in ["/: status 303, expected 200", "/Smoke-Check-Missing-Page: status 303, expected 404"]:
            self.assertIn(f"error: {problem}", result.stderr)

    def test_only_a_realms_own_page_sends_a_referrer_to_tetsu(self):
        # A realm's login alone sends `same-origin`, so a posted form carries its real origin (ADR-0059).
        site = self.site(headers=dict(GOOD, **{"Referrer-Policy": "same-origin"}))
        self.assertIn("error: /: referrer-policy is 'same-origin'", smoke(site.url).stderr)
        admin = self.realm(pages={"/": 303, "/login": 200, "/Smoke-Check-Missing-Page": 303})
        result = smoke(admin.url, "--serves", "admin")
        self.assertIn("error: /login: referrer-policy is 'no-referrer'", result.stderr)
        self.assertNotIn("error: /: referrer-policy", result.stderr)

    def test_a_realms_root_must_send_visitors_to_log_in(self):
        elsewhere = self.realm(locations={"/": "/admin"})
        result = smoke(elsewhere.url, "--serves", "staff")
        self.assertEqual(result.returncode, 1)
        self.assertIn("error: /: sends visitors to '/admin', expected '/login'", result.stderr)

    def test_the_sites_login_must_lead_to_staff_login(self):
        # A staff origin set to the site's own would send `/login` to itself, forever.
        looping = self.site(locations={"/login": "/login"})
        result = smoke(looping.url)
        self.assertEqual(result.returncode, 1)
        self.assertIn(f"error: /login: sends visitors to '/login', expected '{LOGIN}'", result.stderr)

    def test_the_sites_check_needs_the_staff_login(self):
        site = self.site()
        result = run(["python3", SCRIPT, site.url, "--interval", "0"], ROOT)
        self.assertEqual(result.returncode, 2)
        self.assertIn("usage: scripts/smoke_check.py", result.stderr)

    def test_the_site_serves_no_realms_pages(self):
        site = self.site(pages={"/": 200, "/login": 303, "/settings/profile": 200})
        result = smoke(site.url)
        self.assertEqual(result.returncode, 1)
        self.assertIn("error: /settings/profile: status 200, expected 404", result.stderr)

    def test_bad_arguments_print_the_usage(self):
        for args in [
            [],
            ["tetsu.example.com"],
            ["https://x", "--wait"],
            ["https://x", "--wait", "soon"],
            ["https://x", "--retries", "3"],
            ["https://x", "--serves", "ops"],
            ["https://x", "--serves"],
            ["https://x", "--login", "app.example.com/login"],
        ]:
            result = run(["python3", SCRIPT, *args], ROOT)
            self.assertEqual(result.returncode, 2, args)
            self.assertIn("usage: scripts/smoke_check.py", result.stderr)
