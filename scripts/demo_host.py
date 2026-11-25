#!/usr/bin/env python3
"""The host provider, in one place (ADR-0092): every call the deploy makes to it goes through here, so
moving to another host changes this file and the deploy environment's values, not the workflow. The host is
Render today (ADR-0012), and its token comes from HOST_TOKEN.

check asks the host about each realm's service, given as <service>=<origin>: each is named, its own, readable
with the token, and served at that origin. deploy sends one image to each named service. database-url
prints a service's DATABASE_URL, for the steps that act as the application. install puts the host's command
line tool in a folder, only when its checksum matches the pinned one.

usage: scripts/host.py check <id>=<origin>... | deploy <digest> <id>... | database-url <id> | install <folder>
"""

import hashlib
import io
import json
import os
import re
import subprocess
import sys
import zipfile
from pathlib import Path

DIGEST = re.compile(r"ghcr\.io/heavymedl-labs/tetsu@sha256:[0-9a-f]{64}")
POSTGRES = re.compile(r"postgres(ql)?://.+")
# Render's API and its command line tool, pinned by version and checksum; each is overridden only by tests.
API = os.environ.get("HOST_API", "https://api.render.com/v1")
CLI_URL = os.environ.get("HOST_CLI_URL", "https://github.com/render-oss/cli/releases/download")
CLI_VERSION = os.environ.get("HOST_CLI_VERSION", "2.31.0")
CLI_SHA256 = os.environ.get(
    "HOST_CLI_SHA256", "1cbebf1c0e083fd4410df7096fd76205bd5d6e81c7893f2587b34f1cfe2cdf85"
)
TIMEOUT = 30


class HostError(Exception):
    pass


def token():
    value = os.environ.get("HOST_TOKEN", "")
    if not value:
        raise HostError("HOST_TOKEN must be set to the host's API token")
    return value


def named(services):
    """Fails unless the token is set and each service is named and its own, so no realm's service is left
    on an old image while another is deployed twice."""
    token()
    if not all(services):
        raise HostError("every realm's service must be named in the deploy's environment; deploying nothing")
    if len(set(services)) != len(services):
        raise HostError("each realm's service must be its own, named once; deploying nothing")


def check(pairs):
    """Fails unless a deploy could reach each realm's service, given as `service=origin`: named, its own,
    readable from the host with the token, and served at that realm's origin, so a run that would fail or
    deploy the wrong service stops before anything changes."""
    split = [pair.partition("=") for pair in pairs]
    if not all(origin for _, _, origin in split):
        raise HostError("every realm's origin must be set in the deploy's environment; deploying nothing")
    named([service for service, _, _ in split])
    for service, _, origin in split:
        body = fetched(
            f"{API}/services/{service}", f"Authorization: Bearer {token()}", "Accept: application/json"
        )
        try:
            answer = json.loads(body or b"null")
            served = answer.get("service", answer)["serviceDetails"]["url"]
        except (ValueError, AttributeError, KeyError, TypeError):
            raise HostError(f"{service} could not be read from the host; deploying nothing") from None
        if served != origin:
            raise HostError(f"{service} serves {served}, not {origin}; deploying nothing")


def deploy(digest, services):
    """Deploys `digest` to each service in turn, waiting for each, once each is named and its own."""
    named(services)
    environment = dict(os.environ, RENDER_API_KEY=token(), RENDER_CLI_DISABLE_ANALYTICS="1")
    for done, service in enumerate(services):
        command = ["render", "deploys", "create", service, "--image", digest, "--wait", "--confirm"]
        if subprocess.run([*command, "-o", "text"], env=environment, check=False).returncode != 0:
            ran = ", ".join(services[:done]) or "no service"
            raise HostError(f"{service} did not deploy; {ran} runs the new image")


def fetched(url, *headers):
    """The body at `url`, with each header, or none when it cannot be fetched."""
    command = ["curl", "-fsSL", "--max-time", str(TIMEOUT)]
    for header in headers:
        command += ["-H", header]
    answer = subprocess.run([*command, url], capture_output=True, check=False)
    return answer.stdout if answer.returncode == 0 else None


def database_url(service):
    """The DATABASE_URL a service runs with."""
    body = fetched(
        f"{API}/services/{service}/env-vars/DATABASE_URL",
        f"Authorization: Bearer {token()}",
        "Accept: application/json",
    )
    try:
        value = json.loads(body or b"{}").get("value", "")
    except (ValueError, AttributeError):
        value = ""
    if not isinstance(value, str) or not POSTGRES.fullmatch(value):
        raise HostError(f"{service}'s DATABASE_URL could not be read as a Postgres URL")
    return value


def install(folder):
    """Puts the host's command line tool in `folder` as `render`, if its download matches its checksum."""
    name = f"cli_{CLI_VERSION}_linux_amd64.zip"
    zipped = fetched(f"{CLI_URL}/v{CLI_VERSION}/{name}") or b""
    if hashlib.sha256(zipped).hexdigest() != CLI_SHA256:
        raise HostError(f"{name} does not match its pinned checksum; installing nothing")
    with zipfile.ZipFile(io.BytesIO(zipped)) as archive:
        tool = archive.read(f"cli_v{CLI_VERSION}")
    target = Path(folder) / "render"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(tool)
    target.chmod(0o755)


def main(args):
    command, rest = (args[0], args[1:]) if args else ("", [])
    valid = (
        (command == "deploy" and len(rest) > 1 and DIGEST.fullmatch(rest[0])) or (command == "check" and rest)
    ) or (command in ("database-url", "install") and len(rest) == 1)
    if not valid:
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    try:
        if command == "check":
            check(rest)
        elif command == "deploy":
            deploy(rest[0], rest[1:])
        elif command == "database-url":
            print(database_url(rest[0]))
        else:
            install(rest[0])
    except HostError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
