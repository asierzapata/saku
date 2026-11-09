---
name: create-pr
description: Prepare a concise saku PR description, or open a PR when the user asks for one. Use for new PRs and for describing the changes on an existing branch.
---

# Prepare a reviewable PR

Help a reviewer understand the purpose, the resulting behavior, and the evidence without reconstructing the work session. Preparing a description does not authorize a push or any write to GitHub.

## Establish the actual change

Inspect the branch, the working tree, the commits, and any existing PR (`gh pr view`). Determine the base from the request, the existing PR, or the branch history. It is usually `main`. Ask only if it stays ambiguous. Compare committed changes against the merge base, and keep uncommitted work out of the PR unless the user says otherwise.

If the diff mixes unrelated purposes, explain the grouping that would make review easier. Do not split a published branch or rewrite history on your own.

Base every claim on observed work, the diff, and context the user gave you. Do not invent a cause, a rejected alternative, a measured improvement, or a rationale. If essential information is missing, mark the description as unfinished.

## Review and verify first

Before writing the description, run the `review-change` skill on the complete diff unless a review in this session already covered it. Fix the defects that fall within the requested scope, or report them to the user before publishing. Mention a finding in the description only when the reviewer needs it.

CI runs `cargo build` and `cargo test` on every PR to `main`. Before opening the PR, run at least the checks that can catch a plausible failure in the change, such as `cargo test -p <crate>`, `cargo test -p saku-tdo --no-default-features`, and `cargo clippy`. Report which checks ran, which failed, and which did not run.

## Write the title and description

Use the repository's title convention: the affected tools or crates in parentheses, then an imperative summary, for example `(tdo) Add blocked filter to view today` or `(sync) Make KV pull resilient to decryption failures`.

Use two sections:

- **Purpose** — a short paragraph with the concrete problem and why it needs a change.
- **Changes** — a few bullets that group the meaningful changes by purpose. One bullet is enough for a small PR.

Add a **Verification** line with what was actually checked and the outcome. Add a risk, migration step, or compatibility note only when the reviewer needs it. Always call out changes to the on-disk format, the sync protocol, or the CLI output, because they affect existing data, other devices, and agents that parse the output.

Do not enumerate files, describe each function, reproduce the diff, or narrate the investigation. Use complete sentences, and remove repetition before shortening sentences.

## Deliver or publish

For a description-only request, return the title and body ready to use. When the user asks you to open the PR, check for an existing one first, push only the intended branch, and open it with `gh pr create` ready for review, not as a draft, unless the user asks for a draft. An existing PR does not authorize overwriting its body unless the user asked for that update. Read back the title, base, and draft status, and report the URL with any unresolved item.
