---
name: write-doc
description: Write or update saku documentation for a specific reader and task, such as a guide under documentation/, an RFC, a README, or the saku-integration skill. Use when documentation is requested or a code change makes an existing document inaccurate.
---

# Write useful documentation

Work out who will read the document and what they need to understand, decide, or do. Saku has two kinds of reader: people using or developing the tools, and agents that read the docs and skills to operate them. Write so both can act on the text.

Choose the smallest change that serves the reader. Update the document that already covers the subject, and create a new one only when no existing document is the right place. The main places are:

- `README.md` for the suite overview and installation.
- `documentation/` for architecture, setup guides, and per-tool docs. `PHILOSOPHY.md` holds design intent and `architecture.md` holds shared conventions.
- `documentation/rfc-*.md` for proposals and design decisions.
- `skills/` for the skills that saku ships to users' agents. Keep them short, because they are loaded into an agent's context.

## Ask about what the repository does not show

When you only correct a document after a code change, ask only when the answer would change what the document tells the reader.

Before you write a new document or rewrite an existing one, ask about what the repository does not show. Send up to three of these questions in one message, chosen by the type of document, and skip any that the conversation or the sources already answer. Tell the user that rough answers are enough.

- For any document: "What made you want this document?" or "Which part of this will be wrong in a year, and what will make it wrong?"
- For an explanation or RFC: "What is the one idea the reader must leave with?", "What looks wrong but is intentional?", or "Which alternatives did you rule out, and why?"
- For a how-to: "What did you have to figure out that nothing told you?" or "Where can someone do damage that is hard to undo, such as losing data or breaking sync?"
- For a reference: "Which values or rules here did you choose, and which are fixed by the code?"

Wait for the answers. If the user asks you to go ahead without answering, explain once why the answers matter and repeat only the most important question. After that, write with what you have.

If your agent runs without a user to ask, write with what the sources give you and mark the gaps as described below.

## Write

Start with the answer, or with what the reader has to do, and then add the reasoning and detail this reader needs. Match the form to the content: short numbered steps for an operation, prose for a concept, and tables or exact signatures for a contract. Do not give all three the same outline.

Base every claim on code you have read, on documents that are still current, or on what the user told you. Do not supply a reason, an intention, a past decision, or a measurement that no source gives, even as a hedged guess. When something is uncertain, present it as uncertain.

If the document needs information you cannot get, write `TODO(author): <what is missing>` in its place, and do not present the document as finished while a marker remains.

Add examples and links where the reader needs them. Command examples must match the current CLI, so check them against `--help` or the code. When a detailed reference already exists, link to it instead of copying it.

Use plain words and complete sentences. Before you hand over, cut repeated ideas, generic introductions, and summaries that repeat the body. Keep exact contract details, warnings about real operations, and anything the user explicitly asked for.

## Hand over

Report only the result and the gaps that matter: the `TODO` markers that remain, the recommendations you wrote yourself, and the claims that rest only on what the user remembers. Ask the user to confirm each of them.
