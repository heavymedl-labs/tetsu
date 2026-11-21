#!/usr/bin/env python3
"""Runs a local Mailpit that `cargo tetsu serve` sends to, so its email shows at http://localhost:8025.

Mailpit answers only on this machine, over implicit TLS with a certificate for `localhost` and a login, as a
real provider would (ADR-0074). Both are made in `.tools/mailpit/` the first time and kept.

While Mailpit runs, a marked block in `.env` points Tetsu at it. Stopping the script, by Ctrl+C, closing its
terminal, or kill, stops Mailpit and takes the block out again, keeping any other edit, so mail goes back to
/ops/mailbox. Only kill -9 or a crash leaves the block, which the next run replaces. It never overrides mail
settings in `.env`, or exported where it starts, nor a running Mailpit's; settings exported only where `serve`
runs are yours to unset. Only the addresses given reach Mailpit, as TETSU_MAIL_ALLOW, so give your operator's
to see their invite, which is never kept anywhere else.

usage: scripts/mailpit.py <address> [address ...]
"""

import os
import re
import secrets
import signal
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / ".tools" / "bin" / "mailpit"
FILES = ROOT / ".tools" / "mailpit"
ENV = ROOT / ".env"
USER = "tetsu"
SMTP = 1465
WEB = 8025
# The block names the process that wrote it, so a second run tells a running Mailpit's from a killed one's.
START = "# Local Mailpit for process {pid}, set by scripts/mailpit.py until it stops.\n"
END = "# End of local Mailpit.\n"
BLOCK = re.compile(r"\n?# Local Mailpit for process (\d+), set by .*?\n.*?" + re.escape(END), re.DOTALL)
# Tetsu's mail settings (crates/tetsu/src/settings.rs), which .env and the environment must not set already.
SETTING = re.compile(r"TETSU_(SMTP|MAIL)_\w+")
MAIL = re.compile(r"^\s*(TETSU_(?:SMTP|MAIL)_\w+)\s*=", re.MULTILINE)


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


def running(pid):
    """Whether `pid` is a run of this script, not just any process: after kill -9 or a reboot, a left block's
    process id may belong to something else."""
    shown = subprocess.run(
        ["ps", "-o", "command=", "-p", str(pid)], capture_output=True, text=True, check=False
    )
    return "mailpit.py" in shown.stdout


def without_blocks(text, pids=None):
    """`.env` with the blocks of `pids` taken out, or with every block if none are named."""
    return BLOCK.sub(lambda block: "" if pids is None or int(block[1]) in pids else block[0], text)


def save(text):
    """Writes `.env` whole or not at all, so an interrupt cannot cut it short. The copy is made with `.env`'s
    permissions from the start, since it holds a password, and replaces the file a symlinked `.env` names."""
    target = ENV.resolve()
    partial = target.with_name(f"{target.name}.mailpit-partial")
    partial.unlink(missing_ok=True)
    descriptor = os.open(partial, os.O_WRONLY | os.O_CREAT | os.O_EXCL, target.stat().st_mode & 0o777)
    try:
        with os.fdopen(descriptor, "w") as file:
            file.write(text)
        partial.replace(target)
    finally:
        partial.unlink(missing_ok=True)


def stop(_signal, _frame):
    """A closed terminal or kill stops Mailpit as Ctrl+C does, so the block still goes."""
    raise KeyboardInterrupt


def next_steps(address):
    return f"""Mail for {address} now shows at http://localhost:{WEB}

In another terminal, from the repository's root:
  cargo tetsu setup
  cargo tetsu operator list
With no operator, make yours, and its invite email arrives:
  cargo tetsu operator create "Your Name" {address}
With one, give it this address and send a new invite:
  cargo tetsu operator email <id> {address}
  cargo tetsu operator invite <id>
Then run cargo tetsu serve, open the invite from Mailpit, and join: the login and passkey notices follow.

Ctrl+C stops Mailpit and takes its settings out of .env."""


def refusal():
    """Why the local Mailpit cannot start here, or nothing."""
    if not BINARY.exists():
        return "Mailpit is not installed. Run scripts/install-tools.sh"
    if not ENV.exists():
        return "there is no .env yet. Run: cp .env.example .env"
    text = ENV.read_text()
    live = [int(pid) for pid in BLOCK.findall(text) if running(int(pid))]
    if live:
        return f"a local Mailpit already runs, as process {live[0]}. Stop it first."
    if "DATABASE_URL" in os.environ:
        return "DATABASE_URL is exported, so serve takes no mail settings from .env. Unset it to use .env's."
    # Even an empty one, which serve still prefers to .env's.
    exported = sorted(name for name in os.environ if SETTING.fullmatch(name))
    for where, own in (
        (".env already sets", sorted(set(MAIL.findall(without_blocks(text))))),
        ("the environment sets", exported),
    ):
        if own:
            return f"{where} {', '.join(own)}. Unset them while you use the local Mailpit."
    return None


def main(addresses):
    if not addresses:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    refused = refusal()
    if refused:
        print(f"error: {refused}", file=sys.stderr)
        return 1
    before = without_blocks(ENV.read_text())
    FILES.mkdir(parents=True, exist_ok=True)
    key, cert = certificate()
    auth, password = login()
    settings = [
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
    for hangup in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(hangup, stop)
    save(before.rstrip("\n") + "\n\n" + START.format(pid=os.getpid()) + "\n".join(settings) + "\n" + END)
    try:
        print(next_steps(addresses[0]), flush=True)
        return subprocess.run(command, check=False).returncode
    except KeyboardInterrupt:
        return 0
    finally:
        save(without_blocks(ENV.read_text(), {os.getpid()}))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
