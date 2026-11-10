# Saku — Philosophy

> 作 (saku): to make, to create, to craft.

Saku is a small, local-first toolbox for managing work, reading documents, and automating explicit routines. It should remove overhead, not introduce another system to maintain.

## Product Boundaries

| Tool | Responsibility | Boundary |
|---|---|---|
| `tdo` | What needs doing? | Manage tasks; do not execute their contents. |
| `mdr` | Make Markdown pleasant to read in the terminal. | Read ordinary files; no document database or memory service. |
| `sch` | Run a configured command at a configured time. | No task picking, prompt construction, or implicit execution. |

Only `tdo` is implemented and under active development. `mdr` and `sch` are future scope: no preparatory implementations or dependencies are needed while the focus is on tasks.

## Human-Centered, Open to Agents

Humans, scripts, and agents should be able to use the same operations and data. Agent access is an interface property, not a separate product.

The intended interaction convention is:

- Bare commands open a human-oriented TUI in an interactive terminal.
- Explicit subcommands perform predictable operations and exit.
- Structured output and file/stdin input are available where appropriate.
- Interactive actions are explicit; scripting paths do not unexpectedly prompt.
- Both interfaces share the same domain operations.

In the current checkout, bare `tdo` opens its local TUI in an interactive terminal and prints usage otherwise. Use explicit subcommands for noninteractive work.

## Lightweight by Default

Adding a task should stay fast. Prefer few concepts, direct commands, and optional configuration. Task operations and document reading should not require a background service merely to access local data.

Keep local data usable offline. Replication is infrastructure for tasks, not a reason to make local operations depend on network availability.

Share abstractions only when real implementations need them. Do not build a universal framework in anticipation of future tools.

## Explicit Automation

A recurring task is a reminder. A scheduled command performs an action. Neither should implicitly become the other.

The planned scheduler starts with macOS, machine-local jobs, and a required per-job missed-run policy. Its run output belongs in execution history, not in a personal knowledge system. Detailed scheduler design waits until work on that tool begins.

## Ordinary Documents

Long-form explanations and decisions belong in ordinary Markdown files. The planned reader makes those files easier to consume, including tables, code, and diagrams, without owning their creation or synchronization.
