---
name: review-change
description: Review a saku change for actionable defects, such as correctness, data loss, sync or storage-format breakage, and CLI contract changes. Use when the user asks for a code review, or before preparing or opening a PR when no review has covered its diff yet.
---

# Review the change

The review must not be biased by the reasoning that produced the change. If your agent can start a subagent or a fresh session, run the review there and pass it only the base branch and a short statement of what the change must achieve, not how it was built. If it cannot, review the diff as if someone else wrote it, and ignore what you remember about why each line exists.

Report what you find without editing files. The agent or person who asked for the review decides which fixes to apply.

## Scope the review

Work out what you were asked to review and which branch the change is based on. It is usually `main`, but a stacked branch may have another base. Compare against the merge base, for example `git diff $(git merge-base main HEAD)`. Include uncommitted changes only when the request covers them.

Read the diff, the code that uses what changed, and the code the change relies on. Go further only to follow a specific effect. Read `CLAUDE.md` and `documentation/architecture.md` for the conventions the change must respect. For a change to storage or sync, also read `documentation/sync-architecture.md` and the RFCs under `documentation/`.

When the request lists checks that already ran, such as `cargo test`, do not run them again just to confirm they pass. They cover only the code they exercise. When the diff can affect code outside them, such as another caller of a changed function or another crate in the workspace, check that code yourself.

## What to look for

Prioritize problems that could realistically happen:

- **Correctness.** Wrong results, panics on `unwrap` or indexing with user input, off-by-one errors, and unhandled `Result`s.
- **Data integrity.** The on-disk format is a versioned KV map. A change to a serialized struct, a storage key, or the format version can make existing data unreadable or silently drop fields. Check `serde` defaults and renames, migrations between versions, and that the tombstone and rename pattern (`renamed_to`, `previous_key`) still holds.
- **Sync.** Merges use last-write-wins per entry, followed by rename reconciliation and reference repair. Look for changes that make merge order matter, break timestamps, or leave dangling references after a merge. Check behavior when one device runs an older version.
- **Crypto and secrets.** Keys, keychain access, and encrypted payloads must not be logged, written in plain text, or weakened.
- **CLI contract.** Humans and agents both parse tdo output. A change to a command name, flag, exit code, or the JSON or CSV output can break scripts and agents. Flag it unless the change says it is intentional.
- **Tests.** The changed behavior, including its error paths, is covered by a test that would fail without the change. For storage or sync changes, look for a test that loads data in the previous format or merges with an older peer.
- **Feature flags.** Code behind the `sync` feature must still compile and pass tests without it: `cargo test -p saku-tdo --no-default-features`.

Before you report a problem, check that the surrounding code does not already handle it, and make sure you can explain what causes it and what it breaks. Confirm it by reading the code. Write a reproduction, such as a test, only when the code alone cannot settle the question. Do not invent benchmarks or test results.

## Report

For each finding, give its severity, the file and line, the scenario in which it fails, and the smallest fix that solves it. Do not list every file you reviewed, add a section on what the change does well, or add empty severity sections.

Leave style preferences out unless they cause a concrete maintenance problem or the request asks for them. Clippy already covers most of them. A comment or doc that describes a contract wrongly is a defect, however, because the next person will rely on it.

If you find no defects, say so in one sentence. Either way, mention anything important that you could not check. A clean review is not an approval, so do not say the change is ready to release.
