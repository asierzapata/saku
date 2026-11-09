---
name: context7
description: Retrieve up-to-date documentation for libraries and crates, such as clap, serde, or tokio, through the Context7 API. Use when you need a library's API, setup, configuration, or examples, or when its API may have changed since your training data.
---

# Context7

Query the Context7 API with `curl` to get current documentation instead of relying on possibly outdated training data. No API key is needed for basic, rate-limited use. For Rust crates, the generated API docs on docs.rs are also authoritative for an exact version.

## 1. Find the library ID

```bash
curl -s "https://context7.com/api/v2/libs/search?libraryName=LIBRARY_NAME&query=TOPIC" | jq '.results[0]'
```

- `libraryName` (required): the library to search for, for example `clap` or `serde`.
- `query` (required): the topic, used to rank results.

The response includes `id` (for the next step), `title`, `description`, and `totalSnippets`.

## 2. Fetch the documentation

```bash
curl -s "https://context7.com/api/v2/context?libraryId=LIBRARY_ID&query=TOPIC&type=txt"
```

- `libraryId` (required): the `id` from step 1.
- `query` (required): the specific topic.
- `type` (optional): `json` (default) or `txt`, which is easier to read.

## Example

```bash
curl -s "https://context7.com/api/v2/libs/search?libraryName=clap&query=derive+subcommands" | jq '.results[] | {id, title, totalSnippets}'
curl -s "https://context7.com/api/v2/context?libraryId=<id>&query=derive+subcommands&type=txt"
```

## Gotchas

- **The first result may be the wrong library.** Common names match several projects, so check `title` and `description` before using the `id`.
- **Check the version.** Compare what the docs describe with the version in `Cargo.toml` or `Cargo.lock`. Context7 usually documents the latest release.
- **Rate limits.** If you get a 429, wait before retrying, and do not call the endpoint in a loop.
- **Sparse libraries.** When `totalSnippets` is small, read the official docs or docs.rs directly.
- **Encode queries.** Use `+` or `%20` for spaces.
- **IDs change.** If a known ID returns 404, search again.
