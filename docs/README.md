# Harbor Documentation

This directory contains current project documentation. Historical decisions, specifications, and completed implementation tickets live under [`.grimoire/`](../.grimoire/).

## Where to Look

| Question                                     | Source of truth                                                        |
| -------------------------------------------- | ---------------------------------------------------------------------- |
| What should be implemented next?             | [`roadmap.md`](roadmap.md)                                             |
| Is a VT sequence or behavior supported?      | [`protocol/checklist.md`](protocol/checklist.md)                       |
| How does the widget runtime work now?        | [`architecture/widget-runtime.md`](architecture/widget-runtime.md)     |
| Why was an architectural choice made?        | [`.grimoire/adr/`](../.grimoire/adr/)                                  |
| What is the measured memory baseline?        | [`performance/memory-baseline.md`](performance/memory-baseline.md)     |
| How should profiling be run?                 | [`performance/profiling-guide.md`](performance/profiling-guide.md)     |
| What performance work remains?               | [`performance/optimization-plan.md`](performance/optimization-plan.md) |
| What evidence is required before completion? | [`validation.md`](validation.md)                                       |

## Document Responsibilities

### Roadmap

The roadmap owns priorities, dependencies, deliverables, and release gates. It does not duplicate protocol-level checklists, profiling captures, or completed implementation histories.

### Protocol Checklist

The checklist owns feature-coverage claims. A checked item means the implementation is clear and supported by focused tests or reproducible runtime evidence. Roadmap status never overrides checklist evidence.

### Architecture Documents

Architecture documents describe the current design and its invariants. They do not preserve superseded designs; ADRs explain why decisions changed.

### Performance Documents

Measured baselines are immutable evidence. Profiling procedures explain how to reproduce evidence. Optimization plans contain only open or explicitly accepted work.

### Grimoire Artifacts

ADRs, specifications, and tickets preserve decision and delivery history. Their status fields reflect implementation reality, but they are not the current project roadmap.

## Precedence

When documents disagree, use this order:

1. Accepted or implementing ADRs for architectural decisions
2. Current architecture documents for implemented structure
3. Protocol checklist for feature coverage
4. Roadmap for execution priority
5. Root README for orientation only

## Maintenance Rules

- Keep all project documentation in English.
- Store a fact in one canonical document and link to it elsewhere.
- Do not copy volatile test counts into overview documents.
- Do not mark work complete without the evidence defined in [`validation.md`](validation.md).
- Move historical measurements out of active plans rather than rewriting them.
- Update links when files move; `scripts/check_docs.py` validates local Markdown links and language policy.
- Use `scripts/checklist_summary.py` to calculate protocol coverage instead of maintaining counts manually.
