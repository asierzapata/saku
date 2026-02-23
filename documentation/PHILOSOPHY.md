# Saku — Philosophy

> *作 (saku): to make, to create, to craft.*

---

## The Bet

Saku is built on one idea that most productivity tools haven't fully confronted:

**In 2026, "you" is not one person. It's a human-agent team.**

AI agents can write code, run tests, research topics, draft documents, manage files. They can work while you sleep. They can parallelize work you'd have to do serially. The question is no longer just "what should I do next?" — it's "what should we do next, and who does which part?"

No existing productivity tool was designed for that entity. Saku is.

---

## What We're Building

Saku is a **suite of focused terminal tools that serve as the shared context layer between human intention and agent execution.**

Not a task manager. Not a note-taking app. Not a calendar. Each tool in the suite is one dimension of a shared workspace that both a human and an AI agent can read from and write to, using a common vocabulary and a common protocol: the CLI.

The suite:

| Tool | Role |
|---|---|
| `tdo` | Work queue. What needs to be done, and who does it. |
| `nte` | Knowledge base. What we know, what we've learned. |
| `cal` | Time constraints. When things need to happen. |
| `hbt` | Human rhythms. Habits only the human tracks. |
| `tmr` | Time spent. How long work actually takes. |
| `bkm` | References. What we've saved, what we return to. |

Together they form a **local-first productivity OS** — one the human operates through a keyboard and an agent operates through a shell.

---

## What a "Task" Means to Us

In most todo apps, a task is a reminder — a string of text that jogs your memory when you see it. The assumption is that *you* already know how to do it; you just needed to not forget.

In Saku, a task is a **work order**. It carries enough context for whoever executes it — human or agent — to do so without asking follow-up questions. The title says what. The notes say how, and what done looks like. The project says why. The dependencies say what has to happen first.

A well-formed task is self-contained. An agent can pick it up, execute it, and leave notes on what it did — without a synchronous conversation.

This changes how we think about every field:

- **Title** — clear enough to execute, not just to recognize
- **Notes** — the spec, not the afterthought
- **Deadline** — a hard constraint, not a suggestion
- **Depends on** — an execution graph, not just a memory aid
- **Inbox** — the human review queue, not just the unorganized pile
- **Logbook** — the audit trail, not just a pat on the back

---

## The Inbox as the Dialogue Surface

The most important reframe in Saku's model:

**The inbox is where the human-agent conversation happens.**

When an agent discovers something while working — a bug to fix, a decision to make, a task to delegate — it creates a task in the inbox. The human processes the inbox: authorizing work (moving to today), rejecting it (deleting), or delegating it back (adding notes and tagging for agent execution).

When the human has something to offload to an agent, they write it with enough spec in the notes and tag it accordingly. The agent reads its queue.

Neither party needs to be online at the same time. The inbox is the async channel.

---

## The Logbook as the Audit Trail

The logbook isn't just "things I finished." It's **how we know what happened and why.**

When an agent completes a task, it leaves execution notes: what it changed, what it found, what it decided. When a human reviews the logbook, they're doing a lightweight code review of the agent's decisions — spot-checking, approving, catching mistakes.

This is the human-in-the-loop pattern, and it's critical. An agent with no accountability is dangerous. An agent whose every action is logged and reviewable is a colleague.

---

## Design Principles

**Terminal-first.** The CLI is the universal interface. Humans type into it; agents call it programmatically. Both get the same tool, the same behavior, the same exit codes. No special agent API needed.

**Local-first.** Data lives in `~/.local/share/`. Human-readable JSON. No internet required, no account required, no vendor lock-in. Users own their data.

**Fast by default.** Sub-10ms startup. No async overhead, no daemon, no background sync on every command. Fast enough to be called hundreds of times per day by an agent without friction.

**Composable, not monolithic.** Each tool does one thing. Tools can reference each other's data but remain independently useful. The Unix philosophy, applied to a human-agent workspace.

**Legible to both principals.** Every output format — the terminal display, the JSON export, the structured exit codes — must be equally readable by a human skimming it and a program parsing it. These are not two different audiences. They're two modes of the same user.

**Opinionated, not configurable.** We make choices so users don't have to. The data model, the visual language, the command structure — all of it is decided. Configuration options are a last resort, not a default.

---

## What We're Not

**Not a team collaboration tool.** Saku is for one human and their agents. It's not Jira, it's not Linear, it's not Asana. No shared workspaces, no comments, no @mentions.

**Not cloud-native.** Sync is supported via saku-sync, but it's opt-in and self-hosted. The core tool works entirely offline. We don't want to know what's in your task list.

**Not a general-purpose automation platform.** We're not building webhooks, plugin systems, or a marketplace. Integration happens through the CLI and the file system, like every Unix tool before us.

**Not trying to replace your calendar or email.** Saku is the workspace, not the world. It coordinates work; it doesn't replace the tools where that work is communicated or time-boxed.

---

## The Long Game

The developer who uses AI agents for software engineering today is a preview of how most knowledge workers will work within a few years. Saku is built for that person, and by extension, for that future.

The AGPL license is intentional. This is infrastructure. It should stay open, stay forkable, stay inspectable — both by humans who want to understand it and by agents who need to reason about it.

If we get this right, Saku becomes the standard for how a human and their agents share a workspace: a common vocabulary, a common store, a common rhythm of capture → review → execute → log.

That's the bet.
