#!/usr/bin/env python3
"""Runs a local Mailpit that `cargo tetsu serve` sends to, so its email shows at http://localhost:8025.

Mailpit answers only on this machine, over implicit TLS with a certificate for `localhost` and a login, as a
real provider would (ADR-0074). Both are made in `.tools/mailpit/` the first time and kept.

While Mailpit runs, a marked block in `.env` points Tetsu at it, and Ctrl+C takes the block out again, so mail
goes back to /ops/mailbox. It never overrides mail settings of your own: comment those out first. Only the
addresses given reach Mailpit, as TETSU_MAIL_ALLOW, so give your operator's to see their invite, which is
never kept anywhere else.

usage: scripts/mailpit.py <address> [address ...]
"""

import os
import re
import secrets
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
START = "# Local Mailpit, written by scripts/mailpit.py and taken out when it stops.\n"
END = "# End of local Mailpit.\n"
# Tetsu's mail settings (crates/tetsu/src/settings.rs), which .env must not set outside the block.
MAIL = re.compile(r"^\s*(TETSU_SMTP_\w+|TETSU_MAIL_\w+)\s*=", re.MULTILINE)


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


def without_block(text):
    """`.env` with any block a run left behind taken out."""
    return re.sub(r"\n?" + re.escape(START) + r".*?" + re.escape(END), "", text, flags=re.DOTALL)


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


def main(addresses):
    if not addresses:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    if not BINARY.exists():
        print("error: Mailpit is not installed. Run scripts/install-tools.sh", file=sys.stderr)
        return 1
    if not ENV.exists():
        print("error: there is no .env yet. Run: cp .env.example .env", file=sys.stderr)
        return 1
    before = without_block(ENV.read_text())
    own = sorted(set(MAIL.findall(before)))
    if own:
        print(
            f"error: .env already sets {', '.join(own)}. Comment them out while you use the local Mailpit.",
            file=sys.stderr,
        )
        return 1
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
    ENV.write_text(before.rstrip("\n") + "\n\n" + START + "\n".join(settings) + "\n" + END)
    try:
        print(next_steps(addresses[0]), flush=True)
        return subprocess.run(command, check=False).returncode
    except KeyboardInterrupt:
        return 0
    finally:
        ENV.write_text(before)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
