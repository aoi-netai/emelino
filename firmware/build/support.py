"""SDK-independent command logging and build-result publication."""

import hashlib
import json
from pathlib import Path
import shlex
import subprocess


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_json(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    temporary.replace(path)


class CommandLog:
    def __init__(self, path):
        self.path = path
        self.file = None

    def __enter__(self):
        self.file = self.path.open("w")
        return self

    def __exit__(self, *exception):
        self.file.close()

    def run(self, command, timeout=120, **kwargs):
        command = list(map(str, command))
        self.file.write("$ " + shlex.join(command) + "\n")
        self.file.flush()
        result = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                text=True, timeout=timeout, **kwargs)
        self.file.write(result.stdout)
        self.file.flush()
        if result.returncode:
            raise ValueError(f"command failed ({result.returncode}); see {self.path}\n{result.stdout}")
        return result.stdout


def record_artifacts(output, metadata):
    """Publish the success marker only after every declared artifact exists."""
    artifacts = metadata["artifacts"]
    if not artifacts or len(set(artifacts)) != len(artifacts):
        raise ValueError("invalid firmware artifact list")
    for name in artifacts:
        path = Path(name)
        if path.is_absolute() or ".." in path.parts or output not in (output / path).resolve().parents:
            raise ValueError("firmware artifact escapes output: " + name)
    manifest = dict(metadata, schema_version=1,
                    sha256={name: sha256(output / name) for name in artifacts})
    write_json(output / "firmware.json", manifest)
    return manifest
