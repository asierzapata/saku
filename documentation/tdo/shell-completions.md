# Shell Completions for tdo

The `tdo` CLI supports shell completions for all major shells through the `tdo completion` command.

## Supported Shells

- Bash
- Zsh
- Fish
- PowerShell
- Elvish

## Installation

### Bash

**Option 1: System-wide (requires sudo)**
```bash
tdo completion bash | sudo tee /usr/share/bash-completion/completions/tdo
```

**Option 2: User-specific**
```bash
mkdir -p ~/.local/share/bash-completion/completions
tdo completion bash > ~/.local/share/bash-completion/completions/tdo
```

**Option 3: Dynamic loading (add to ~/.bashrc)**
```bash
eval "$(tdo completion bash)"
```

Then restart your shell or run `source ~/.bashrc`.

---

### Zsh

**Option 1: Oh-My-Zsh (if you use it)**
```zsh
mkdir -p ~/.oh-my-zsh/completions
tdo completion zsh > ~/.oh-my-zsh/completions/_tdo
```

**Option 2: System-wide (requires sudo)**
```zsh
tdo completion zsh | sudo tee /usr/local/share/zsh/site-functions/_tdo
```

**Option 3: User-specific**
```zsh
mkdir -p ~/.zsh/completion
tdo completion zsh > ~/.zsh/completion/_tdo

# Add this to your ~/.zshrc if not already present:
fpath=(~/.zsh/completion $fpath)
autoload -U compinit && compinit
```

Then restart your shell or run `source ~/.zshrc`.

---

### Fish

Fish completions are automatically discovered from the standard location:

```fish
mkdir -p ~/.config/fish/completions
tdo completion fish > ~/.config/fish/completions/tdo.fish
```

Completions will be available immediately (no restart needed).

---

### PowerShell

**Option 1: Dynamic loading (add to your PowerShell profile)**
```powershell
# Find your profile location:
echo $PROFILE

# Add this line to your profile:
Invoke-Expression (tdo completion powershell | Out-String)
```

**Option 2: One-time activation (current session only)**
```powershell
tdo completion powershell | Out-String | Invoke-Expression
```

---

### Elvish

```elvish
mkdir -p ~/.config/elvish/lib
tdo completion elvish > ~/.config/elvish/lib/tdo.elv

# Add this to your ~/.config/elvish/rc.elv:
use tdo
```

Then restart your shell.

---

## What Gets Completed

Once installed, shell completions provide:

### Commands
- `tdo <TAB>` - Shows all available commands: `today`, `inbox`, `add`, `create`, etc.
- `tdo create <TAB>` - Shows subcommands: `area`, `project`
- `tdo show <TAB>` - Shows subcommands: `area`, `project`, `tag`
- `tdo list <TAB>` - Shows subcommands: `areas`, `projects`, `tags`

### Flags and Options
- `tdo add --<TAB>` - Shows all available flags: `--today`, `--tomorrow`, `--project`, etc.
- `tdo add -<TAB>` - Shows short flags: `-p`, `-a`, `-t`, `-n`
- Flag descriptions are shown in supported shells

### Help Text
- Completions include descriptions for each command and flag
- Use your shell's completion preview feature to see descriptions

---

## Troubleshooting

### Bash: Completions not working
1. Verify `bash-completion` is installed:
   - Ubuntu/Debian: `sudo apt install bash-completion`
   - macOS (Homebrew): `brew install bash-completion@2`
2. Make sure your `.bashrc` sources bash-completion
3. Try the dynamic loading option instead

### Zsh: Completions not working
1. Make sure `compinit` is called in your `.zshrc`
2. Try rebuilding the completion cache: `rm -f ~/.zcompdump && compinit`
3. Verify the completion file is in your `fpath`: `echo $fpath`

### Fish: Completions not working
1. Check the file was created: `ls ~/.config/fish/completions/tdo.fish`
2. Try reloading completions: `fish_update_completions`

---

## Testing Your Installation

After installation, test your completions:

```bash
# Type this and press TAB:
tdo <TAB>

# You should see all commands listed

# Test flag completion:
tdo add --<TAB>

# You should see flags like --today, --tomorrow, --project, etc.
```

---

## Future Enhancements

Currently, completions are static (based on the CLI structure). Future versions may include:

- Dynamic completion of project names
- Dynamic completion of area names
- Dynamic completion of tag names
- Dynamic completion of task numbers

These would require additional implementation for context-aware completions.
