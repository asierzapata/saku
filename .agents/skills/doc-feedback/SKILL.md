---
name: doc-feedback
description: Review a saku document, such as a file under documentation/, a README, an RFC, or a skill under skills/, and report findings on accuracy, structure, prose, and substance. Use when the user asks for feedback on a doc, asks whether a doc is good, or asks what they would change about it. For writing a doc, use the write-doc skill.
---

# Doc feedback

Review the document and report findings the author can act on. This skill reviews and does not rewrite. Apply fixes only if the user explicitly asks.

## Ground rules

1. **Verify accuracy findings before reporting them.** Search the code for every file path, crate, command, flag, struct, and field that the doc names. Run `<tool> --help` or read the clap definitions to check command examples. Never report that something does not exist, or praise examples as correct, without having checked. If prose and a table disagree, use the code to decide which is right.
2. **Read the siblings.** Read the doc's neighbors in the same folder and the docs it links to, such as `architecture.md` and `PHILOSOPHY.md`. Duplication and coverage can only be judged against the set.
3. **Stay humble on substance.** You can verify craft, but you cannot verify knowledge you do not have. Phrase substance findings as questions to the author, for example "is the 15s sync interval a decision or a placeholder?". For each section, ask whether it could be regenerated from the repository. If it could, suggest a link. If it could not and it has no source, ask the author to confirm it. Do not report section order as a defect, because you do not know the author's reader. Ask who they picture reading it instead.
4. **Severity discipline.** Use two severities: **blocker** (wrong, misleading, or unusable) and **improvement**. Lead with blockers, and do not bury structural findings under nitpicks.

## What to check

- **Accuracy.** Commands, flags, paths, versions, and status tables match the code and `Cargo.toml`. Status claims such as "Shipping" or "Planned" match what exists under `crates/`.
- **Structure.** The doc starts with the answer or with what the reader has to do. A how-to uses steps, an explanation uses prose, and a reference uses exact contracts.
- **Prose.** Plain words, complete sentences, and no generic introductions, repeated ideas, or summaries that repeat the body.
- **Agent readers.** A skill or a doc that agents load must be short, must say when to use it, and must give commands an agent can run without guessing.
- **Substance.** The doc contains knowledge that is not obvious from the code, such as reasons, constraints, and pitfalls.

## Report

1. **Verdict.** One short paragraph: is the doc sound, and what is the most important fix?
2. **Blockers**, each with file and line, what is wrong, and the evidence you verified.
3. **Improvements**, grouped by category, each with a concrete fix. For prose, show the rewrite.
4. **Questions for the author**, listed separately so they are not mistaken for defects.
5. **What works**, so a fix pass does not flatten the doc's strengths.

If asked to fix the doc, apply craft fixes directly, but never answer a substance question by inventing an answer. Leave those to the author.
