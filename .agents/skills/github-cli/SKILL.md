---
name: github-cli
description: Use the GitHub CLI (gh) for GitHub operations on the saku repository, such as viewing or creating PRs and issues, reading review comments, and checking CI runs.
---

# GitHub CLI

Use `gh` from the shell for GitHub operations instead of building URLs by hand or asking the user to use the web UI. Use plain `git` for local work such as commits, branches, and diffs. Writing to GitHub, such as creating a PR, commenting, or merging, needs the user's request.

## Common commands

| Task | Command |
| --- | --- |
| PR for the current branch | `gh pr view --json number,title,state,url,isDraft` |
| Specific PR | `gh pr view 123` |
| PR diff | `gh pr diff 123` |
| PR checks | `gh pr checks` |
| PR review comments | `gh api repos/{owner}/{repo}/pulls/123/comments --jq '.[] \| {author: .user.login, path, line, body}'` |
| Create a draft PR | `gh pr create --draft --base main --title "..." --body-file <file>` |
| Issues | `gh issue list`, `gh issue view 123`, `gh issue create` |
| CI runs | `gh run list`, `gh run view <id> --log-failed` |
| Auth | `gh auth status` |

## Practices

- Check whether a PR already exists before creating one: `gh pr view --json url 2>/dev/null`.
- Use `--json` with `--jq` when you need to parse the output.
- Pass a multi-line body with `--body-file`, or with a quoted heredoc (`<<'EOF'`) so the shell does not expand variables inside it.
- Report the URL after creating a PR or issue.
- The default branch is `main`, and CI is the `Rust` workflow, which runs `cargo build` and `cargo test`. Use `gh run view <id> --log-failed` to read a failure.

## Gotchas

- **Auth expires.** If commands fail with 401, run `gh auth status` and ask the user to run `gh auth login`. Do not retry the same command.
- **JSON fields are camelCase**, for example `isDraft` and `mergedAt`. A wrong field name fails with a list of valid fields.
- **`gh pr view` without a number uses the current branch**, which fails on `main` or on a branch without a PR.
- **`gh api` calls count against the rate limit.** Prefer the `gh pr` and `gh issue` subcommands.
