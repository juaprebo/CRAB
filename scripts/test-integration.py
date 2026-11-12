import os
import pathlib
import subprocess
import tempfile

repo = pathlib.Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="crab-integration-") as temporary:
    root = pathlib.Path(temporary)
    alsa = root / "alsa.conf"
    alsa.write_text("pcm.!default { type null }\n")
    commands = root / "bin"
    commands.mkdir()
    for name in ["wpctl", "pactl", "amixer"]:
        shim = commands / name
        shim.write_text("#!/bin/sh\nexit 1\n")
        shim.chmod(0o755)
    environment = os.environ.copy()
    environment.update(
        CRAB_TEST_SANDBOX=temporary,
        ALSA_CONFIG_PATH=str(alsa),
        PATH=str(commands) + os.pathsep + environment["PATH"],
        XDG_CONFIG_HOME=str(root / "config"),
        XDG_STATE_HOME=str(root / "state"),
        XDG_CACHE_HOME=str(root / "cache"),
    )
    subprocess.run(
        ["cargo", "test", "--locked", "--offline", "rename_and_stats_survive_application_restart", "--", "--ignored", "--test-threads=1"],
        cwd=repo,
        env=environment,
        check=True,
    )
