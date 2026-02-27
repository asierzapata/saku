# Saku AI Integration

Saku uses a two-part system to steer AI agents toward productive task management:

1. **CLAUDE.md snippet** — A directive added to your project instructions that tells the AI to proactively use `tdo` as a shared task ledger. This is what makes the AI _want_ to use it.
2. **Skill** (`saku-integration`) — The full command reference, loaded on demand. This is what teaches the AI _how_ to use it.

Both parts are needed. The snippet without the skill leaves the AI without enough detail. The skill without the snippet means the AI only uses `tdo` when explicitly asked.

## Setup

### Step 1: Install the skill

Choose one method:

**Claude Code (recommended)**
```bash
mkdir -p ~/.claude/skills/
cp -r skills/saku-integration ~/.claude/skills/
```

**Cursor**
```bash
mkdir -p .cursor/skills/
cp -r skills/saku-integration .cursor/skills/
```

**Per-project**
```bash
# Claude Code
mkdir -p <project>/.claude/skills/
cp -r skills/saku-integration <project>/.claude/skills/

# Cursor
mkdir -p <project>/.cursor/skills/
cp -r skills/saku-integration <project>/.cursor/skills/
```

### Step 2: Add the CLAUDE.md snippet

Append this to your project's `CLAUDE.md` (or `rules.md`, `.cursorrules`, etc.):

```markdown
## Saku productivity suite

`tdo` is your shared task ledger with the user. Always use it to track work that spans sessions.

Break down initiatives into tasks. Record milestones. Report progress with `tdo done`. This keeps the user informed across sessions.

Load `/saku-integration` before first use for the full command reference.

Key commands:
- `tdo add "task" -p project` · `tdo add "sub" --parent <id>` · `tdo depend <id> --on <id>`
- `tdo done <id>` · `tdo view today` · `tdo view project "name"`

TodoWrite = your short-term step tracking. `tdo` = shared, lasting work.
```

Adapt the language for your agent platform if needed (e.g., Cursor uses different conventions for rules files).

## How it works

The snippet sits in the agent's always-loaded context (~120 tokens). It frames `tdo` as the medium/long-term shared ledger — distinct from built-in task tracking tools which handle short-term step-by-step execution.

When the agent encounters work that spans sessions or matters to the user, the snippet pushes it to:
1. Load the `/saku-integration` skill (on-demand, ~400 tokens)
2. Use `tdo` commands to create, track, and complete tasks
3. Keep the user informed of progress through the shared task store

## Verification

Ask your AI agent:

> "What do you know about tdo?"

It should reference the shared task ledger concept from the snippet. Then ask:

> "Break down this feature into tasks and track them"

It should load the saku-integration skill and start using `tdo add`, `tdo depend`, etc.

## Updating

```bash
cd /path/to/saku && git pull
cp -r skills/saku-integration ~/.claude/skills/  # or your target location
```

## License

AGPL-3.0-or-later
