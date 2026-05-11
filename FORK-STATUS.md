# Fork status — `xiboplayer/arexibo`

This is a fork of [`birkenfeld/arexibo`](https://github.com/birkenfeld/arexibo),
the original Rust/Qt6 Xibo signage player for Linux (primarily Raspberry Pi).
Our fork adds release-channel infrastructure and a CI gate we want on every
change. Some of our patches are good upstream candidates; others are
fork-local by design.

## Divergence from upstream, as of 2026-04-21

Run `git log --oneline upstream/master..origin/master` to see the live list.

### Category A — already PR'd upstream (awaiting maintainer review)

| Our commit (squashed) | Title | Upstream PR |
|---|---|---|
| `c8b098a` | `fix: 2 pre-existing clippy errors` | [birkenfeld/arexibo#30](https://github.com/birkenfeld/arexibo/pull/30) |

### Category B — upstream-PR candidates, queued behind #30

These are genuine behaviour bug fixes that upstream would likely benefit
from. Holding them until we see how birkenfeld reviews #30 — to avoid
flooding the maintainer with parallel PRs before establishing a signal
on their merge appetite.

| Our commit | Fix | Why upstream-valuable |
|---|---|---|
| `be21c38` | `HashingWriter::write` hashes only bytes actually written | Silent MD5 corruption on every short-write path — broke resource integrity for any user |
| `3e19742` | XMR socket gets `set_rcvtimeo(30_000)` + `assert!` → `bail!` in `process_msg` | Player wedged forever if XMR publisher dropped silently; panic on malformed frame would kill the whole player |
| `b96062a` | `CB_STOPSHELL` reads `kill_mode` from `arg1` not `arg2` | Every CMS-triggered StopShell was a silent no-op since the feature landed (C++ side passes it as `arg1`; Rust side was reading `arg2` which is always 0) |
| `5211aa5` | `collect_once` plumbs real `last_command_success` | `lastCommandSuccess: false` was hardcoded so the CMS thought every player command failed regardless of outcome |

Trigger to open the bundle PR: either (a) birkenfeld merges PR #30, signalling they're receptive, or (b) ≥2 weeks with no response, after which we chase with a friendly follow-up.

### Category C — fork-local by design (do NOT upstream)

Our release channel + packaging machinery, tied to `dl.xiboplayer.org`
and the `xibo-players` GitHub org's release workflow. Upstream has no
use for these.

| Commit(s) | What |
|---|---|
| `0d773d2` | feat: add RPM and DEB packaging |
| `e613383` | docs(README): SRPMs rebuild + verify section |
| `86e9630` | docs(README): binary builds DO ship (our tag pipeline) |
| `19caf47` | fix(rpm): correct day-of-week in `%changelog` dates |
| `3e01206` | ci: GitHub Actions workflows + Dependabot |
| `2a124c5` | security: CodeQL analysis workflow |
| `09b0619` | ci(test): cargo test + clippy gate (our fork's specific config) |
| `60f66f4` | ci: `rust-toolchain.toml` from `1.75.0` pin → `stable` — upstream keeps the MSRV pin |
| `7f175eb` | ci: force rustup stable on PATH (no longer needed after `60f66f4` but harmless) |

### Category D — upstream-PR candidates with unclear value

Possibly useful upstream, possibly not. Defer until someone actually asks.

| Commit | What |
|---|---|
| `8cbd33c` | fix: sort changelog in descending chronological order — upstream doesn't ship a changelog file so this is arguably fork-specific |
| `04f8023` | security: pin GitHub Actions to SHA digests — broadly good practice, but upstream runs few GH Actions so low impact |
| `9fc1d4a` | feat: add `arexibo.service` systemd user unit — nice to have for distro packagers; defer until a packager asks |

### Category E — fork-specific audit markers

| Commit | What |
|---|---|
| `abcf1f6` | scaffold(audit): mark 3 arexibo bugs as TODOs — from overnight-audit-2026-04-21 |

### Category F — automatic

Dependabot bumps (`f7ec095`, `30b6756`, etc.) — replay as they appear upstream too.

## Rules for future contributors

1. **Before adding a new commit on `master`**, decide its category.
   Record the intent in the commit body ("Fork-local: our release machinery"
   vs "Upstream candidate: behaviour fix").
2. **When rebasing on upstream**, drop commits that have become
   redundant (upstream implemented the same fix differently) — do NOT
   silently carry them as duplicates.
3. **When opening an upstream PR**, do it from our fork directly:
   `xiboplayer/arexibo` is the GitHub-recognised fork of
   `birkenfeld/arexibo` (`linuxnow/arexibo` redirects to us — the
   linuxnow repo was transferred to the org during the account
   consolidation; compare-URLs only work from the `xibo-players`
   namespace).
4. **Never force-push `master`** without announcing in this file's
   history. The 2026-04-21 rebase-on-upstream was the first and
   should be the last for a long time.

## Maintainer context

Primary point of contact for this fork: Pau Aliagas <pau@xiboplayer.org>.
Discussion of upstreaming strategy: private in
`xiboplayer/xiboplayer-compliance` (tracking issue #TBD).
