#!/usr/bin/env python3
"""Runs a local Mailpit that Tetsu sends to, so its email shows at http://localhost:8025.

Mailpit answers only on this machine, over implicit TLS with a certificate for `localhost` and a login, as a
real provider would (ADR-0074). Both are made in `.tools/mailpit/` the first time and kept.

While Mailpit runs, `.env.mailpit` holds `.env`'s settings, with the mail settings that reach Mailpit in
place of any of yours, for `cargo tetsu --env mailpit` (ADR-0073). Only you can read it, since it holds the
login, and it goes when the script stops, by Ctrl+C, Ctrl+\\, closing its terminal, or kill. Only kill -9 or a
crash leaves it, and the next run then asks you to remove it. `.env` itself is never touched. Only the
addresses given reach Mailpit, so give your operator's to see their invite, which Tetsu itself never keeps.
Mailpit's inbox, kept in `.tools/mailpit/`, opens with no login to anyone using this machine. Two first runs
started at the same instant can leave a key and certificate that do not match, and Mailpit then cannot start:
remove `.tools/mailpit/key.pem` and `cert.pem`, and the next run makes them again.

usage: scripts/mailpit.py <address> [address ...]
"""

import os
import re
import secrets
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / ".tools" / "bin" / "mailpit"
FILES = ROOT / ".tools" / "mailpit"
ENV = ROOT / ".env"
SETTINGS = ROOT / ".env.mailpit"
USER = "tetsu"
SMTP = 1465
WEB = 8025
# Settings the environment must not set, since it wins over any file, even when empty: the mail settings
# (crates/tetsu/src/settings.rs), and DATABASE_URL, which makes Tetsu refuse mail settings from a file.
EXPORTED = re.compile(r"DATABASE_URL|TETSU_(SMTP|MAIL)_\w+")
# A mail setting in `.env`, which the script's own take the place of.
MAIL = re.compile(r"^\s*TETSU_(SMTP|MAIL)_\w+\s*=.*\n?", re.MULTILINE)


# How the script is stopped, besides kill -9: Ctrl+C, Ctrl+\\, a closed terminal, and kill.
STOPS = (signal.SIGINT, signal.SIGQUIT, signal.SIGHUP, signal.SIGTERM)
# Set by the first stop, after which any other is ignored.
STOPPING = []
# How long Mailpit has to stop when asked, before it is killed.
GRACE = 5
REFUSED_EXISTING = ".env.mailpit already exists: a Mailpit runs, or one was killed. If none runs, remove it."


def certificate():
    """A key and a certificate for `localhost`, an end entity as rustls accepts, made once."""
    key, cert = FILES / "key.pem", FILES / "cert.pem"
    if not (key.exists() and cert.exists()):
        subprocess.run(
            [
                *("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "3650"),
                *("-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost"),
                *("-addext", "basicConstraints=critical,CA:FALSE", "-keyout", str(key), "-out", str(cert)),
            ],
            check=True,
            capture_output=True,
        )
    return key, cert


def login():
    """The password Mailpit checks, made once and kept in its login file, readable only by its owner."""
    auth = FILES / "auth"
    if not auth.exists():
        descriptor = os.open(auth, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, "w") as file:
            file.write(f"{USER}:{secrets.token_hex(16)}\n")
    return auth, auth.read_text().strip().split(":", 1)[1]


def refusal():
    """Why the local Mailpit cannot start here, or nothing."""
    if not BINARY.exists():
        return "Mailpit is not installed. Run scripts/install-tools.sh"
    if not ENV.exists():
        return "there is no .env yet. Run: cp .env.example .env"
    if SETTINGS.exists():
        return REFUSED_EXISTING
    exported = sorted(name for name in os.environ if EXPORTED.fullmatch(name))
    if exported:
        return (
            f"the environment sets {', '.join(exported)}, which Tetsu prefers to any file. Unset them first."
        )
    return None


def stop(_signal, _frame):
    """The first stop ends the run, which then stops Mailpit and removes its settings file. Any later one,
    such as Ctrl+C pressed twice, is ignored, so nothing cuts that cleanup short."""
    if STOPPING:
        return
    STOPPING.append(True)
    raise KeyboardInterrupt


def ended(child):
    """Stops Mailpit, asking first and killing it if it has not stopped in time."""
    try:
        os.kill(child, signal.SIGTERM)
        deadline = time.monotonic() + GRACE
        while os.waitpid(child, os.WNOHANG) == (0, 0):
            if time.monotonic() > deadline:
                os.kill(child, signal.SIGKILL)
                os.waitpid(child, 0)
                return
            time.sleep(0.05)
    except (ChildProcessError, ProcessLookupError):
        return


def next_steps(address):
    return f"""Mail for {address} now shows at http://localhost:{WEB}

In another terminal, from the repository's root, use Mailpit's settings with --env mailpit:
  cargo tetsu --env mailpit setup
  cargo tetsu --env mailpit operator list
With no operator, make yours, and its invite email arrives:
  cargo tetsu --env mailpit operator create "Your Name" {address}
With one, give it this address and send a new invite:
  cargo tetsu --env mailpit operator email <id> {address}
  cargo tetsu --env mailpit operator invite <id>
Then run cargo tetsu --env mailpit serve, open the invite from Mailpit, and join: the login and passkey
notices follow.

Ctrl+C stops Mailpit and removes .env.mailpit."""


def main(addresses):
    if not addresses:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    refused = refusal()
    if refused:
        print(f"error: {refused}", file=sys.stderr)
        return 1
    FILES.mkdir(parents=True, exist_ok=True)
    key, cert = certificate()
    auth, password = login()
    settings = [
        "# The local Mailpit's, written by scripts/mailpit.py from .env, and removed when it stops.",
        MAIL.sub("", ENV.read_text()).rstrip("\n"),
        "TETSU_SMTP_HOST=localhost",
        f"TETSU_SMTP_PORT={SMTP}",
        f"TETSU_SMTP_USERNAME={USER}",
        f"TETSU_SMTP_PASSWORD={password}",
        f"TETSU_SMTP_CA_FILE={cert}",
        "TETSU_MAIL_FROM=tetsu@tetsu.localhost",
        f"TETSU_MAIL_ALLOW={','.join(addresses)}",
    ]
    command = [str(BINARY), "--smtp", f"127.0.0.1:{SMTP}", "--listen", f"127.0.0.1:{WEB}"]
    command += ["--smtp-tls-cert", str(cert), "--smtp-tls-key", str(key), "--smtp-require-tls"]
    command += ["--smtp-auth-file", str(auth), "--database", str(FILES / "mailpit.db")]
    command += ["--disable-version-check"]
    for each in STOPS:
        signal.signal(each, stop)
    made, child = False, None
    try:
        # Made only if absent, so a run started at the same moment never overwrites another's, and readable
        # only by its owner from the start, since it holds the login. Only a file this run made is removed. A
        # stop is held while the file is made and recorded, so none lands between the two; tests cannot time a
        # signal to that instant, so this rests on reading.
        held = signal.pthread_sigmask(signal.SIG_BLOCK, STOPS)
        try:
            descriptor = os.open(SETTINGS, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            made = True
        finally:
            signal.pthread_sigmask(signal.SIG_SETMASK, held)
        with os.fdopen(descriptor, "w") as file:
            file.write("\n".join(settings) + "\n")
        print(next_steps(addresses[0]), flush=True)
        # Started with stops held too, so a stop always finds Mailpit's process to end; Mailpit itself starts
        # with none held.
        held = signal.pthread_sigmask(signal.SIG_BLOCK, STOPS)
        try:
            child = os.posix_spawn(command[0], command, os.environ, setsigmask=())
        finally:
            signal.pthread_sigmask(signal.SIG_SETMASK, held)
        _, status = os.waitpid(child, 0)
        child = None
        return os.waitstatus_to_exitcode(status)
    except FileExistsError:
        print(f"error: {REFUSED_EXISTING}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        if child is not None:
            ended(child)
        return 0
    finally:
        if made:
            SETTINGS.unlink(missing_ok=True)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
