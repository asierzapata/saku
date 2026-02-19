# Saku Integration Skill

This skill helps AI agents (like Claude, Cursor, Windsurf, etc.) effectively use Saku's task management tool `tdo`.

## Installation

### Method 1: Using Vercel Skills CLI (Recommended)

If you have the Vercel AI SDK skills CLI installed:

```bash
npx skills add https://github.com/asierzapata/saku.git
```

### Method 2: Manual Installation for Claude Desktop

```bash
# Copy skill to Claude's skills directory
mkdir -p ~/.claude/skills/
cp -r skills/saku-integration ~/.claude/skills/
```

### Method 3: Manual Installation for Cursor

```bash
# Copy skill to Cursor's skills directory
mkdir -p .cursor/skills/
cp -r skills/saku-integration .cursor/skills/
```

### Method 4: Repository-Specific Installation

For project-specific skill installation:

```bash
# In your project root
mkdir -p .claude/skills/
cp -r /path/to/saku/skills/saku-integration .claude/skills/

# Or for Cursor
mkdir -p .cursor/skills/
cp -r /path/to/saku/skills/saku-integration .cursor/skills/
```

## What This Skill Provides

The skill teaches AI agents how to:

1. **Understand Saku's Architecture**
   - Data model (Areas → Projects → Tasks)
   - Storage format and locations
   - Core philosophy and design principles

2. **Execute Commands Effectively**
   - Complete command reference with examples
   - Best practices for automation
   - Error handling and exit codes

3. **Implement Common Workflows**
   - Daily planning assistance
   - Project management
   - Task capture from natural language
   - Weekly reviews

4. **Handle Edge Cases**
   - Missing projects/areas
   - Date parsing ambiguities
   - Concurrent access patterns
   - Error recovery

5. **Build Integrations**
   - Example code for task capture bots
   - Daily digest generators
   - Project dashboards
   - Smart scheduling systems

## Usage Examples

Once installed, AI agents will be able to help you with commands like:

**User:** "Add a task to review the PR tomorrow"
**Agent:** 
```bash
tdo add "Review the PR" --tomorrow
```

**User:** "Show me what I need to do today"
**Agent:**
```bash
tdo today
```

**User:** "Create a new project called 'Website Redesign' in my Work area"
**Agent:**
```bash
tdo create area "Work"  # If needed
tdo create project "Website Redesign" --area Work
```

**User:** "Move task 15 to next Friday"
**Agent:**
```bash
tdo move 15 --on 2026-02-27  # Using ISO date for reliability
```

## Benefits

- **Faster Interactions**: Agents understand Saku's command patterns instantly
- **Fewer Errors**: Agents know about common pitfalls and how to avoid them
- **Better Suggestions**: Agents can propose workflow improvements based on best practices
- **Consistent Behavior**: All agents using this skill will interact with Saku the same way

## Updating the Skill

To update to the latest version:

```bash
# Pull latest Saku repository
cd /path/to/saku
git pull

# Reinstall skill
npx skills add https://github.com/asierzapata/saku.git --force

# Or manually copy again
cp -r skills/saku-integration ~/.claude/skills/
```

## Verification

To verify the skill is installed correctly, ask your AI agent:

> "Do you know how to use Saku's tdo command?"

The agent should respond with knowledge about tdo's capabilities and command structure.

## Troubleshooting

### Skill Not Recognized

If the agent doesn't seem to have the skill:

1. **Check installation location**
   ```bash
   ls ~/.claude/skills/saku-integration/  # For Claude
   ls .cursor/skills/saku-integration/    # For Cursor
   ```

2. **Verify SKILL.md exists**
   ```bash
   cat ~/.claude/skills/saku-integration/SKILL.md | head -20
   ```

3. **Restart your AI agent/editor**

### Agent Not Using Best Practices

If the agent isn't following the skill's guidance:

1. Explicitly reference the skill:
   > "Using the Saku integration skill, help me add a task"

2. Provide feedback to improve future interactions

## Contributing

To improve this skill:

1. Edit `skills/saku-integration/SKILL.md`
2. Test with your AI agent
3. Submit a PR to the Saku repository

## Support

- **Issues**: Report problems with the skill via GitHub Issues
- **Questions**: Check the main Saku documentation or ask in discussions
- **Documentation**: See [SKILL.md](saku-integration/SKILL.md) for the complete skill content

## License

Same as Saku: GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)
