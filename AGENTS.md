# Programming Guidelines

## 1. Think Before Coding

**Don't assume. Don't hide confusion. Surface tradeoffs.**

Before implementing:
- State your assumptions explicitly. If uncertain, ask.
- If multiple interpretations exist, present them - don't pick silently.
- If a simpler approach exists, say so. Push back when warranted.
- If something is unclear, stop. Name what's confusing. Ask.

## 2. Simplicity First

**Minimum code that solves the problem. Nothing speculative.**

- No features beyond what was asked.
- No abstractions for single-use code.
- No "flexibility" or "configurability" that wasn't requested.
- No error handling for impossible scenarios.
- If you write 200 lines and it could be 50, rewrite it.

Ask yourself: "Would a senior engineer say this is overcomplicated?" If yes, simplify.

## 3. Surgical Changes

**Touch only what you must. Clean up only your own mess.**

When editing existing code:
- Don't "improve" adjacent code, comments, or formatting.
- Don't refactor things that aren't broken.
- Match existing style, even if you'd do it differently.
- If you notice unrelated dead code, mention it - don't delete it.

When your changes create orphans:
- Remove imports/variables/functions that YOUR changes made unused.
- Don't remove pre-existing dead code unless asked.

The test: Every changed line should trace directly to the user's request.

## 4. Goal-Driven Execution

**Define success criteria. Loop until verified.**

Transform tasks into verifiable goals:
- "Add validation" → "Write tests for invalid inputs, then make them pass"
- "Fix the bug" → "Write a test that reproduces it, then make it pass"
- "Refactor X" → "Ensure tests pass before and after"

For multi-step tasks, state a brief plan:
```
1. [Step] → verify: [check]
2. [Step] → verify: [check]
3. [Step] → verify: [check]
```

Strong success criteria let you loop independently. Weak criteria ("make it work") require constant clarification.

## 5. Roadmap and Checklist Synchronization

**When a requirement or complete feature is finished, synchronize its implementation status with the project documentation.**

At the end of a requirement, feature, or implementation milestone:

1. Check whether `docs/roadmap.md` exists.
2. Check whether `docs/checklist.md` exists.
3. In each existing document, search for entries related to the completed work.
4. Compare the documented requirements against the actual implementation.
5. Verify that every requirement being marked complete is supported by the code and relevant tests.
6. Update matching entries to reflect the current implementation status.
7. Update descriptions when the implemented behavior, scope, limitations, or verification differs from the original plan.

Rules:

- Only update entries directly related to the completed work.
- Do not mark an item complete merely because code was added.
- Mark an item complete only after its documented acceptance criteria have been satisfied and verified.
- Keep partially completed items explicitly marked as partial or in progress.
- Record meaningful remaining work instead of hiding gaps.
- Preserve the existing formatting, terminology, and status conventions used by each document.
- Do not rewrite unrelated roadmap or checklist content.
- If the implementation reveals that a documented requirement is outdated or incorrect, update it and explain the reason.
- Keep roadmap and checklist updates in the implementation commit by default.
- Use an immediately following `docs` commit only when the synchronization is substantial and independently reviewable.

Before considering the requirement complete, verify:

```text
- Implementation matches the requested behavior.
- Relevant tests or checks pass.
- docs/roadmap.md was inspected when present.
- docs/checklist.md was inspected when present.
- Matching documentation entries accurately reflect the final state.
```

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

<!-- GRIMOIRE:START -->
## Grimoire

This project maintains a grimoire at `.grimoire/`:

- `CONTEXT.md` — domain concepts and terminology (may split into `CONTEXT-[domain].md`)
- `adr/` — architecture decision records
- `spec/` — requirements specifications
- `ticket/` — implementation plans

Consult relevant grimoire files before making design decisions that affect project concepts, architecture, or requirements.
<!-- GRIMOIRE:END -->
