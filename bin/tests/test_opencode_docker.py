from __future__ import annotations

import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]


class OpencodeDockerTests(unittest.TestCase):
    def test_compose_passes_host_path_without_overriding_container_path(self) -> None:
        compose = (ROOT / "opencode" / "docker-compose.yml").read_text()

        self.assertIn('HOST_PATH: "${PATH}"', compose)
        self.assertNotIn('      PATH: "${PATH}"', compose)

    def test_entrypoint_appends_container_path_after_host_path(self) -> None:
        entrypoint = (ROOT / "opencode" / "entrypoint.sh").read_text()

        self.assertIn('container_path="${PATH}"', entrypoint)
        self.assertIn('PATH="${HOST_PATH:+${HOST_PATH}:}${container_path}"', entrypoint)

    def test_image_installs_tmux_client(self) -> None:
        dockerfile = (ROOT / "opencode" / "Dockerfile").read_text()

        self.assertIn(" tmux", dockerfile)

    def test_compose_passes_tmux_environment_and_socket(self) -> None:
        compose = (ROOT / "opencode" / "docker-compose.yml").read_text()

        self.assertIn('TMUX: "${TMUX:-}"', compose)
        self.assertIn('/tmp/tmux-${UID:-1000}:/tmp/tmux-${UID:-1000}', compose)

    def test_compose_uses_host_networking(self) -> None:
        compose = (ROOT / "opencode" / "docker-compose.yml").read_text()

        self.assertIn("network_mode: host", compose)
        self.assertNotIn("ports:", compose)

    def test_entrypoint_preserves_tmux_for_opencode_user(self) -> None:
        entrypoint = (ROOT / "opencode" / "entrypoint.sh").read_text()

        self.assertIn('TMUX="${TMUX:-}"', entrypoint)


if __name__ == "__main__":
    unittest.main()
