# Saku (作) — Terminal Productivity Suite

A suite of focused, fast terminal tools for the human-agent team. Designed to be used by a developer and their AI agents interchangeably — same CLI, same data, same protocol.

See [documentation/PHILOSOPHY.md](documentation/PHILOSOPHY.md) for the full design intent.

---

## The Suite

Tools are organized around the loops of a developer's day, not discrete categories.

### Daily Loop — What's the plan? What happened? What do we hand off?

| Tool | Description | Status |
|---|---|---|
| `tdo` | Task queue. Work orders for human and agent. | **Shipping** v0.5.11 |
| `jrn` | Daily journal. Chronological log of what happened. | Planned |
| `cal` | Calendar. Time constraints and event-driven triggers. | Planned |

### Knowledge Loop — What do we know? Why did we decide this?

| Tool | Description | Status |
|---|---|---|
| `nte` | Notes. Evergreen reference and architecture docs. | Planned |
| `dcs` | Decision log. What was decided, why, and what alternatives were considered. | Planned |

### Work Loop — What am I doing right now? Where did I leave off?

| Tool | Description | Status |
|---|---|---|
| `ctx` | Session context. Saves and restores where you were — for yourself and for agents. | Planned |
| `tmr` | Time tracker. Pomodoro and time-on-task. | Planned |

### Communication Loop — Who am I waiting on?

| Tool | Description | Status |
|---|---|---|
| `msg` | Async waiting. Tracks what you're blocked on from external parties. | Planned |
| `ppl` | People context. Notes about the people you work with. | Planned |

### Human Rhythms — Recurring personal behaviors.

| Tool | Description | Status |
|---|---|---|
| `hbt` | Habit tracker. Daily streaks, GitHub-style heatmap. | Designed |

### Orchestrator

| Tool | Description | Status |
|---|---|---|
| `saku` | Cross-tool context, search, and sync. | Planned |

---

## Installation

### Install tdo

```bash
cargo install --path crates/tdo
```

Or build all tools:

```bash
cargo build --release --workspace
```

The binaries will be available in `target/release/`.

---

## Development

This is a Cargo workspace. To build a specific tool:

```bash
cargo build --release -p tdo
cargo run -p tdo -- view today
```

See [documentation/architecture.md](documentation/architecture.md) for conventions shared across all tools.

---

## AI Agent Integration

Saku is designed as a shared workspace for humans and AI agents. To help agents integrate effectively, install the skill:

```bash
# Using npx (requires Vercel AI SDK skills CLI)
npx skills add https://github.com/asierzapata/saku.git

# Or manually
cp -r skills/saku-integration .claude/skills/
```

See [skills/saku-integration/SKILL.md](skills/saku-integration/SKILL.md) for the agent protocol.

---

## Sync

Saku tools sync across devices via a self-hosted server. See [documentation/sync-setup.md](documentation/sync-setup.md) for setup.

---

## License

GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
