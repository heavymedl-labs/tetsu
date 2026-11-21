#!/usr/bin/env python3
"""Runs a local Mailpit that `cargo tetsu serve` can send to, so every email it sends shows at localhost:8025.

Mailpit answers only on this machine, over implicit TLS with a certificate for `localhost` and a login, as a
real provider would (ADR-0074). Both are made in `.tools/mailpit/` the first time and kept, so the settings it
prints stay right; a missing certificate is made again. It runs until Ctrl+C, and stops with it.

Only the addresses given reach Mailpit, as `TETSU_MAIL_ALLOW`; Tetsu keeps every other message for
/ops/mailbox, and never keeps an invite, so name your operator's address to see theirs.

usage: scripts/mailpit.py [address ...]
"""

import os
import secrets
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / ".tools" / "bin" / "mailpit"
FILES = ROOT / ".tools" / "mailpit"
USER = "tetsu"
SMTP = 1465
WEB = 8025


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


def main(addresses):
    if not BINARY.exists():
        print("error: Mailpit is not installed. Run scripts/install-tools.sh", file=sys.stderr)
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
    ]
    if addresses:
        settings.append(f"TETSU_MAIL_ALLOW={','.join(addresses)}")
    print(f"Mailpit's inbox: http://localhost:{WEB}")
    print("Put these in .env, then start cargo tetsu serve again:\n")
    print("\n".join(settings))
    if not addresses:
        print("\nOnly listed addresses reach Mailpit: run scripts/mailpit.py you@example.com to add yours.")
    print("\nCtrl+C stops Mailpit.", flush=True)
    command = [str(BINARY), "--smtp", f"127.0.0.1:{SMTP}", "--listen", f"127.0.0.1:{WEB}"]
    command += ["--smtp-tls-cert", str(cert), "--smtp-tls-key", str(key), "--smtp-require-tls"]
    command += ["--smtp-auth-file", str(auth), "--database", str(FILES / "mailpit.db")]
    command += ["--disable-version-check"]
    try:
        return subprocess.run(command, check=False).returncode
    except KeyboardInterrupt:
        return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
