# Security Policy

Lumagen is a local desktop app. It stores your AI-provider API keys in the
operating system's credential vault, sends requests to the providers you
configure (fal.ai / OpenRouter), opens `.lumagen` documents and imported image
folders, and writes exported textures to disk. Security reports about any of
that are taken seriously and answered by a human.

This is a small project with one maintainer. The promises below are what one
person can honestly keep.

## Reporting a vulnerability

**Please don't open a public issue for anything security-relevant** — a public
issue tells everyone the hole exists before there's a fix.

- **Preferred:** GitHub Private Vulnerability Reporting — the repo's
  **Security** tab → **Report a vulnerability**, or
  <https://github.com/pixhaus-app/lumagen/security/advisories/new>. It keeps the
  report private and lets a fix ship as a coordinated advisory.
- **If you can't use GitHub:** email **luismmorales@gmail.com** with "Lumagen
  security" in the subject.

A good report includes the version, your OS, and the smallest steps (or a
sample `.lumagen`/image file) that reproduce it.

## What to expect

- **Acknowledgment within 7 days.**
- **A fix or documented mitigation within 30 days** for anything that can hurt a
  user who did nothing wrong (a crafted document or image, a malicious provider
  response). Lower-severity issues are best-effort.
- **No bug bounty** — there's no money behind the project. What's offered is a
  real fix and credit in the release notes if you want it.

## Supported versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | ✅        |
| < 0.1   | ❌        |

Lumagen is pre-1.0: only the latest release gets security fixes.

## Scope

The design intent: Lumagen trusts the local user, and treats everything that
crosses a trust boundary — files it opens and data it receives from the network
— as untrusted input.

**In scope**

- **API-key exposure.** The provider key lives only in the OS credential vault;
  a key that lands in app config on disk, a log file, an exported file, or a
  `.lumagen` document is a bug.
- **Malicious input files.** A crafted `.lumagen` document or an imported image
  that achieves code execution, memory unsafety, a path-traversal read, or an
  unbounded-resource (decompression-bomb) denial of service.
- **Export path traversal.** A material name / filename pattern that writes
  outside the chosen output directory.
- **Malicious provider responses.** Lumagen downloads generated images from URLs
  returned by a provider; a response that induces a write outside its intended
  location, requests to unintended hosts, or a type-sniff bypass is in scope.
- **Supply chain of the release binaries** — see below.

**Out of scope**

- Anything requiring an already-compromised local OS account (code running as
  you can already do what you can).
- Deliberately disclosing your own API key (committing it, screenshotting it).
- Vulnerabilities in the AI providers or in Claude/Anthropic — report those to
  the respective vendor.

## Supply-chain notes

- **Pinned toolchain** (`rust-toolchain.toml`) and a committed `Cargo.lock`; CI
  builds with `--locked`, so the dependency set is fixed and reviewed.
- **`cargo-deny`** gates licenses, RUSTSEC advisories, and crate sources on every
  push/PR and again on a weekly schedule.
- **GitHub Actions are pinned to commit SHAs**, updated via Dependabot one small
  PR at a time.
- **Release binaries carry SLSA build provenance.** Verify a download against the
  workflow that built it:

  ```sh
  gh attestation verify lumagen-<ver>-x86_64-windows.msi --repo pixhaus-app/lumagen
  ```

- **No telemetry, no update checks, no callouts.** The only network traffic is to
  the AI provider you explicitly configure.
