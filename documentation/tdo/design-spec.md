# TDO CLI Design Specification: The "Classic" Layout

This document outlines the visual design specification for the `tdo` CLI application output. The design philosophy is "minimalist and context-aware," inspired by Things 3. It prioritizes clarity and scannability, pushing secondary information to the periphery.

## General Layout Principles

The command output uses a split horizontal layout to balance primary task information with secondary context data.

### Horizontal Composition

Every task line is composed of two distinct sections separated by dynamic whitespace:

1. **Left Section (Primary Information):** Contains the ID, status glyph, and task title. This section is left-aligned.
2. **Right Section (Contextual Information):** Contains the Area and/or Project the task belongs to. This section is **strictly right-aligned against the terminal edge**.

### Vertical Composition

- Tasks are grouped under bold headers based on the current view (e.g., **Today**, **Upcoming**, **Evening**).
- A single empty line separates distinct groups (e.g., between the main "Today" list and the "Evening" bucket).

---

## Component Specifications

### 1. Status Glyphs

Glyphs are minimalist geometric shapes located between the ID and the Title.

| State          | Glyph              | Visual Style                        |
| -------------- | ------------------ | ----------------------------------- |
| **Incomplete** | `○` (Empty Circle) | Standard terminal foreground color. |
| **Overdue**    | `●` (Solid Circle) | **Red** color indicating urgency.   |
| **Completed**  | `✓` (Checkmark)    | Dimmed/gray color.                  |

### 2. Task IDs and Titles

- **IDs:** Short numeric identifiers. Right-aligned within a small fixed width (e.g., 3 spaces) so glyphs align vertically.
- **Titles:** The main task text. Standard terminal foreground color.
- **Completed Tasks:** If a task is completed, the entire line (ID, glyph, title, context) should be applied with a **dimmed** and **strikethrough** style.

### 3. The Context Column (Right-Aligned)

This column displays hierarchy information. It must always be pushed to the far right of the terminal window.

**Styling:**
The entire context string, including separators, must be rendered in a **dimmed / dark gray** color so it recedes visually behind the task title.

**Display Logic Rules:**
How the context string is constructed depends on the task's relationships:

- **Rule A (Project & Area):** If a task belongs to a Project, and that Project belongs to an Area.
- _Display Format:_ `{Area Name} / {Project Name}`

- **Rule B (Area Only):** If a task belongs directly to an Area but no specific Project.
- _Display Format:_ `{Area Name}`

- **Rule C (Project Only - Fallback):** If a task belongs to a Project that has no defined Area.
- _Display Format:_ `{Project Name}`

- **Rule D (Inbox/None):** If a task has neither Project nor Area associations.
- _Display Format:_ [Empty string / Nothing displayed]

---

## Color and Typography Palette Summary

| Element                     | Visual Treatment                                             |
| --------------------------- | ------------------------------------------------------------ |
| View Headers                | **Bold text**                                                |
| Standard Task Title         | Standard terminal foreground (e.g., white/light gray)        |
| Standard Task Glyph (`○`)   | Standard terminal foreground                                 |
| Overdue Glyph (`●`)         | **Red** color                                                |
| Context Text (Area/Project) | **Dimmed** color (e.g., dark gray)                           |
| Context Separator (`/`)     | **Dimmed** color                                             |
| Completed Task Line         | **Dimmed** AND ~~strikethrough~~ style applied to whole line |

---

## Mockup Examples (Target Output)

_Note: In these mockups, the text on the far right represents the dimmed, right-aligned context column._

### 1. Today View (Mixed Contexts)

Demonstrates different combinations of Area/Project logic.

```text
$ tdo today

  Today (Feb 15)                                5 tasks

  1  ●  Submit expense report to finance        Work / Admin
  2  ○  Review PR for auth refactor             Work / rust-24m
  3  ○  Call dentist to reschedule              Personal
  4  ○  Buy milk                                Personal / Errands
  5  ○  Write product spec for tdo              Side Projects / tdo

  ─── Evening ───

  6  ○  Read Chapter 5 of Rust book             Personal / Study

```

### 2. Upcoming View (Temporal groupings)

```text
$ tdo upcoming

  Upcoming

  Tomorrow
  7  ○  Team standup prep                       Work
  8  ○  email landlord about lease              Personal / Apartment

  Monday, Feb 17
  9  ○  Rust chapter 6 exercises                Personal / Study
  10 ○  Schedule quarterly review               Work / Admin

```

### 3. Inbox View (No Context)

Tasks in the inbox lack hierarchy, so the right-hand column remains clean and empty.

```text
$ tdo inbox

  Inbox                                         3 tasks

  11 ○  Research SQLite vs JSON again
  12 ○  Find that cool font I saw on Twitter
  13 ○  Ask Sarah about the deadline

```
