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

Not a task manager. Not a note-taking app. Not a calendar. Each tool in the suite covers one recurring loop in a developer's day — the problems they face over and over, in a rhythm — and serves as a shared surface that both human and agent can read from and write to, using a common vocabulary and a common protocol: the CLI.

---

## The Loops

Most productivity suites map tools to categories (tasks, notes, calendar). We map tools to **loops** — the recurring cycles of a developer's work. Tools built around loops stay small, focused, and answerable: each one solves the problems of one loop and no more.

### The Daily Loop
*What's the plan? What happened? What do we hand off?*

This loop runs every day. It includes knowing what's on the plate (`tdo`), recording what happened (`jrn`), and surfacing time constraints from the world outside the team (`cal`). At the end of every day, the human and agent exchange state through these tools — the human reviewing what the agent completed, the agent picking up what the human left for overnight.

### The Knowledge Loop
*What do we know? Why did we decide this?*

This loop is slower — it accumulates over weeks and months. Notes that document how the system works (`nte`). Decisions that record why it was built that way, what alternatives were rejected, and what the expected consequences were (`dcs`). An agent drawing on the knowledge loop doesn't ask questions the team has already answered. It reads first.

### The Work Loop
*What am I doing right now? Where did I leave off?*

This loop is the most granular — minute-to-minute. Saving and restoring the mental model of a work session (`ctx`), so that whether the human resumes after a meeting or the agent resumes after being spawned, the thread is intact. Time tracking for introspection and capacity planning (`tmr`).

### The Communication Loop
*Who am I waiting on? What did I agree to follow up on?*

This loop is the one most often dropped. Work that is blocked on external parties — design reviews, security sign-offs, replies from people outside the team — lives nowhere in most systems. It disappears into email or memory. `msg` tracks what you're waiting on and when it's overdue. `ppl` carries context about the people in your orbit, so decisions and agreements don't get lost.

### Human Rhythms
*What do I do every day, regardless of what's on the task list?*

This loop belongs to the human alone. Habits (`hbt`) track the behaviors that build capability over time — exercise, reading, writing. Agents don't have habits. This is one of the few tools in the suite that's explicitly human-only.

---

## The Suite

| Loop | Tool | Role |
|---|---|---|
| Daily | `tdo` | Work queue. Work orders for human and agent. |
| Daily | `jrn` | Daily journal. Chronological log of what happened. |
| Daily | `cal` | Calendar. Time constraints and event-driven triggers. |
| Knowledge | `nte` | Notes. Evergreen reference and architecture docs. |
| Knowledge | `dcs` | Decision log. What was decided, why, and what alternatives were rejected. |
| Work | `ctx` | Session context. Saves and restores where you were — for yourself and agents. |
| Work | `tmr` | Time tracker. Pomodoro and time-on-task. |
| Communication | `msg` | Async waiting. What you're blocked on from external parties. |
| Communication | `ppl` | People context. Notes about the people you work with. |
| Human Rhythms | `hbt` | Habit tracker. Daily streaks and consistency over time. |
| Orchestrator | `saku` | Cross-tool context, search, and sync. |

Together they form a **local-first productivity OS** — one the human operates through a keyboard and an agent operates through a shell.

---

## What a "Work Item" Means to Us

In most productivity tools, items are reminders — strings of text that jog your memory. The assumption is that *you* already know how to do it; you just needed to not forget.

In Saku, every item in every tool is a **self-contained record**. Not just a label, but enough context for whoever engages with it — human or agent — to do so without asking follow-up questions.

A task in `tdo`: the title says what, the notes say how and what done looks like, the dependencies say what must happen first.
A note in `nte`: evergreen enough that an agent reading it cold has the context it needs.
A decision in `dcs`: the full reasoning — what was chosen, what was rejected, what the consequence is — not just the outcome.
A session in `ctx`: the mental state, not just the task list — where I was in my thinking, not just what's on the list.

This changes the design of every field in every tool. Notes are specs, not afterthoughts. Decisions record reasoning, not just conclusions.

---

## The Inbox as the Dialogue Surface

In `tdo`, the most important reframe:

**The inbox is where the human-agent conversation happens.**

When an agent discovers something while working — a bug to fix, a decision needed, a task to surface — it creates an item in the inbox. The human processes the inbox: authorizing work (moving to today), rejecting it (deleting), or delegating it back (writing spec in notes and tagging for agent execution).

Neither party needs to be online at the same time. The inbox is the async channel between them.

This pattern generalizes across the suite: `jrn` entries from the agent are reviewed by the human in the morning; `dcs` records the human creates are read by the agent before proposing changes; `ctx` saves from one session are read by the next.

---

## The Logbook as the Audit Trail

The logbook in `tdo` — and analogous history in every tool — isn't just "things that got done." It's **how we know what happened and why.**

When an agent completes a task, it leaves execution notes. When a human reviews the logbook, they're doing a lightweight audit of the agent's decisions. This is the human-in-the-loop pattern. An agent with no accountability is a black box. An agent whose every action is logged and reviewable is a colleague.

---

## Design Principles

**Terminal-first.** The CLI is the universal interface. Humans type into it; agents call it programmatically. Both get the same tool, the same behavior, the same exit codes. No special agent API needed.

**Local-first.** Data lives in `~/.local/share/`. Human-readable JSON. No internet required, no account required, no vendor lock-in. Users own their data.

**Fast by default.** Sub-10ms startup. No async overhead, no daemon. Fast enough to be called hundreds of times per day by an agent without friction.

**Composable, not monolithic.** Each tool does one thing, serves one loop. Tools can reference each other's data but remain independently useful. The Unix philosophy, applied to a human-agent workspace.

**Legible to both principals.** Every output format — the terminal display, the JSON export, the structured exit codes — must be equally readable by a human skimming it and a program parsing it. These are not two different audiences. They're two modes of the same user.

**Opinionated, not configurable.** We make choices so users don't have to. The data model, the visual language, the command structure — all decided. Configuration options are a last resort.

**One loop per tool.** When a tool starts solving problems from a different loop, it has grown too large. Split it.

---

## What We're Not

**Not a team collaboration tool.** Saku is for one human and their agents. It's not Jira, Linear, or Asana. No shared workspaces, no comments, no @mentions.

**Not cloud-native.** Sync is supported via saku-sync, but it's opt-in and self-hosted. The core tools work entirely offline. We don't want to know what's in your task list.

**Not a general-purpose automation platform.** We're not building webhooks, plugin systems, or a marketplace. Integration happens through the CLI and the file system, like every Unix tool before us.

**Not trying to replace your calendar or email.** Saku is the workspace, not the world. It coordinates work; it doesn't replace the tools where that work is communicated externally.

---

## The Long Game

The developer who uses AI agents for software engineering today is a preview of how most knowledge workers will work within a few years. Saku is built for that person, and by extension, for that future.

The AGPL license is intentional. This is infrastructure. It should stay open, stay forkable, stay inspectable — by humans who want to understand it and by agents who need to reason about it.

If we get this right, Saku becomes the standard for how a human and their agents share a workspace: a common vocabulary, a common store, a common rhythm across every loop of the working day.

That's the bet.
