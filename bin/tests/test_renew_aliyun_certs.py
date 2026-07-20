from __future__ import annotations

import base64
import hashlib
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "renew-aliyun-certs" / "renew-aliyun-certs"
WRAPPER = ROOT / "bin" / "renew-aliyun-certs"


class RenewAliyunCertsTests(unittest.TestCase):
    def run_script(
        self, *args: str, cwd: Path | None = None
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(SCRIPT), *args],
            cwd=str(cwd or ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )

    def test_help_prints_usage(self) -> None:
        result = self.run_script("--help")
        self.assertEqual(result.returncode, 0)
        self.assertIn("Usage: renew-aliyun-certs", result.stdout)
        self.assertIn("--dry-run", result.stdout)

    def test_bin_wrapper_forwards_to_implementation(self) -> None:
        result = subprocess.run(
            [str(WRAPPER), "--help"],
            cwd=str(ROOT),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Usage: renew-aliyun-certs", result.stdout)

    def test_missing_config_fails_clearly(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            result = self.run_script("--dry-run", cwd=Path(tmp))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Config file not found", result.stderr)

    def write_config(self, directory: Path, extra: str = "") -> Path:
        config = directory / "renew-aliyun-certs.conf"
        config.write_text(
            "\n".join(
                [
                    'DOMAIN="example.com"',
                    'ALT_NAMES=("*.example.com")',
                    'CERT_DIR="./certs/example.com"',
                    'ALIYUN_REGION_ID="cn-beijing"',
                    'ALIYUN_PROFILE="default"',
                    'ACME_DIRECTORY_URL="https://acme-staging-v02.api.letsencrypt.org/directory"',
                    'DNS_PROPAGATION_SECONDS="1"',
                    'ALB_LISTENER_IDS=("lsn-test-example")',
                    'OSS_CNAME_TARGETS=("test-bucket-example:test-u.example.com")',
                    'SERVER_DEPLOYS=("admin@test.example.com:/tmp:fullchain.pem,privkey.pem:true")',
                    'PROTECTED_SERVER_HOSTS=("prod.example.com")',
                    extra,
                ]
            )
            + "\n"
        )
        return config

    def test_dry_run_prints_configured_targets(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(directory)
            result = self.run_script("--dry-run", cwd=directory)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("DRY RUN", result.stdout)
        self.assertIn("lsn-test-example", result.stdout)
        self.assertIn("test-bucket-example:test-u.example.com", result.stdout)
        self.assertIn("admin@test.example.com", result.stdout)

    def test_production_targets_require_explicit_allow_flag(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(
                directory,
                'SERVER_DEPLOYS+=("admin@prod.example.com:/tmp:fullchain.pem:true")',
            )
            result = self.run_script("--dry-run", cwd=directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("production-like target", result.stderr)

    def test_protected_alb_listener_requires_explicit_allow_flag(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(
                directory,
                'PROTECTED_ALB_LISTENER_IDS=("lsn-test-example")',
            )
            result = self.run_script("--dry-run", cwd=directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("protected ALB listener", result.stderr)

    def test_protected_oss_target_requires_explicit_allow_flag(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(
                directory,
                'PROTECTED_OSS_CNAME_TARGETS=("test-bucket-example:test-u.example.com")',
            )
            result = self.run_script("--dry-run", cwd=directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("protected OSS target", result.stderr)

    def test_dry_run_rejects_invalid_oss_target(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(directory, 'OSS_CNAME_TARGETS=("missing-domain")')
            result = self.run_script("--dry-run", cwd=directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Invalid OSS_CNAME_TARGETS entry", result.stderr)

    def test_dry_run_rejects_invalid_server_deploy(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(directory, 'SERVER_DEPLOYS=("incomplete")')
            result = self.run_script("--dry-run", cwd=directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Invalid SERVER_DEPLOYS entry", result.stderr)

    def test_dry_run_rejects_unknown_server_certificate_file(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_config(
                directory,
                'SERVER_DEPLOYS=("admin@test.example.com:/tmp:missing.pem:true")',
            )
            result = self.run_script("--dry-run", cwd=directory)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Unsupported server certificate file", result.stderr)

    def test_dependency_check_covers_deployment_commands(self) -> None:
        script = SCRIPT.read_text()
        for command in ("install", "scp", "ssh"):
            self.assertIn(f"require_command {command}", script)

    def test_script_does_not_use_unsupported_output_json_flag(self) -> None:
        script = SCRIPT.read_text()
        self.assertNotIn("--output json", script)

    def test_dns_rr_for_root_and_wildcard_identifiers(self) -> None:
        command = (
            'RENEW_ALIYUN_CERTS_TESTING=1 source "$1"; '
            'DOMAIN="example.com"; '
            'dns_rr_for_identifier "example.com"; printf "\\n"; '
            'dns_rr_for_identifier "*.example.com"; printf "\\n"'
        )
        result = subprocess.run(
            ["bash", "-c", command, "bash", str(SCRIPT)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "_acme-challenge\n_acme-challenge\n")

    def test_script_uses_aliyun_minimum_dns_ttl(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("--TTL 600", script)

    def test_acme_request_handles_bad_nonce_retry(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("badNonce", script)
        self.assertIn("Retrying ACME request with fresh nonce", script)

    def test_staging_mode_skips_deployment(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("Staging mode: skipping Aliyun and server deployment", script)

    def test_logs_go_to_stderr_to_keep_command_substitution_clean(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn('printf \'[%s] %s\\n\' "$(date +%H:%M:%S)" "$*" >&2', script)

    def test_cas_upload_uses_explicit_endpoint(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("ALIYUN_CAS_ENDPOINT", script)
        self.assertIn('--endpoint "$ALIYUN_CAS_ENDPOINT" cas UploadUserCertificate', script)

    def test_script_creates_server_compatible_private_key_name(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn(
            'install -m 600 "$CERT_DIR/domain.key" "$CERT_DIR/privkey.pem"', script
        )

    def test_alb_array_parameters_use_force_mode(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn('aliyun --force --profile "$ALIYUN_PROFILE"', script)

    def test_server_deploy_accepts_new_host_keys_noninteractively(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("StrictHostKeyChecking=accept-new", script)
        self.assertIn("BatchMode=yes", script)

    def test_staging_uses_isolated_certificate_directory(self) -> None:
        command = (
            'RENEW_ALIYUN_CERTS_TESTING=1 source "$1"; '
            'CERT_DIR="/tmp/certs/example.com"; STAGING=1; '
            'select_certificate_directory; printf "%s" "$CERT_DIR"'
        )
        result = subprocess.run(
            ["bash", "-c", command, "bash", str(SCRIPT)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "/tmp/certs/example.com/staging")

    def test_cleanup_tracks_sensitive_temporary_files(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("SENSITIVE_TMP_FILES", script)
        self.assertIn('rm -f "${SENSITIVE_TMP_FILES[@]}"', script)

    def test_certificate_is_validated_before_install(self) -> None:
        script = SCRIPT.read_text()
        self.assertIn("validate_downloaded_certificate", script)
        self.assertNotIn('acme_request "$cert_url" \'\' "$CERT_DIR/fullchain.pem"', script)

    def test_account_thumbprint_matches_canonical_jwk_hash(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            cert_dir = Path(tmp) / "certs"
            cert_dir.mkdir()
            account_key = cert_dir / "account.key"
            subprocess.run(
                ["openssl", "genrsa", "-out", str(account_key), "2048"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            command = (
                'RENEW_ALIYUN_CERTS_TESTING=1 source "$1"; '
                'CERT_DIR="$2"; prepare_account_jwk; printf "%s" "$JWK_THUMBPRINT"'
            )
            result = subprocess.run(
                ["bash", "-c", command, "bash", str(SCRIPT), str(cert_dir)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)

            modulus_hex = subprocess.run(
                ["openssl", "rsa", "-in", str(account_key), "-noout", "-modulus"],
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            ).stdout.strip().split("=", 1)[1]
            modulus = bytes.fromhex(modulus_hex)
            jwk = {
                "e": "AQAB",
                "kty": "RSA",
                "n": base64.urlsafe_b64encode(modulus).rstrip(b"=").decode(),
            }
            canonical = json.dumps(jwk, sort_keys=True, separators=(",", ":"))
            expected = base64.urlsafe_b64encode(
                hashlib.sha256(canonical.encode()).digest()
            ).rstrip(b"=").decode()
            self.assertEqual(result.stdout, expected)


if __name__ == "__main__":
    unittest.main()
