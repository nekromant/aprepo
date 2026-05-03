<!--
Sync Impact Report
==================
Version change: 1.0.0 → 1.1.0 (new principle added)
Modified principles: none renamed
Added sections: Principle V. Regress-Resistant Fixes
Removed sections: none
Templates requiring updates:
  - .specify/templates/plan-template.md: ✅ No outdated references
  - .specify/templates/spec-template.md: ✅ No outdated references
  - .specify/templates/tasks-template.md: ✅ No outdated references
  - .specify/templates/agent-file-template.md: ✅ No outdated references
  - .specify/templates/checklist-template.md: ✅ No outdated references
Follow-up TODOs:
  - TODO(RATIFICATION_DATE): Set to actual first adoption date when ratified
-->

# fdroid-repo Constitution

## Core Principles

### I. Think Before Coding

**Non-negotiable**: Before any implementation, state assumptions explicitly.
If uncertain, ask rather than assume. When multiple interpretations exist,
present them — do not pick silently. If a simpler approach exists, say so
and push back when warranted. If something is unclear, stop, name the
confusion, and ask.

**Rationale**: Ambiguity is the primary source of rework. Surfacing tradeoffs
early prevents unnecessary rewrites and ensures alignment between implementer
and requester.

### II. Simplicity First

**Non-negotiable**: Write the minimum code that solves the problem. No
features beyond what was asked. No abstractions for single-use code. No
"flexibility" or "configurability" that was not requested. No error handling
for impossible scenarios. If 200 lines could be 50, rewrite it.

**Rationale**: Overcomplicated code is harder to maintain, review, and reason
about. The quality bar is what a senior engineer would call simple and
focused.

### III. Surgical Changes

**Non-negotiable**: Touch only what you must. Do not "improve" adjacent code,
comments, or formatting. Do not refactor things that are not broken. Match
existing style, even if you would do it differently. If you notice unrelated
dead code, mention it — do not delete it. Remove imports, variables, or
functions only if YOUR changes made them unused.

**Rationale**: Scope discipline keeps diffs focused and reviews fast. Every
changed line MUST trace directly to the user's request.

### IV. Goal-Driven Execution

**Non-negotiable**: Define success criteria before coding. Transform vague
tasks into verifiable goals: "Add validation" becomes "Write tests for
invalid inputs, then make them pass." For multi-step tasks, state a brief
plan with verification checkpoints. Loop until the success criteria are met.

**Rationale**: Weak criteria ("make it work") require constant clarification.
Strong criteria enable independent, verifiable progress and reduce back-and-forth.

### V. Regress-Resistant Fixes

**Non-negotiable**: For every major bug fixed, a test MUST be created that
reproduces the bug before the fix is applied. The test MUST fail before the
fix and pass after. "Major" is defined as any bug that affects user-visible
behavior, corrupts data, crashes the system, or bypasses security controls.
Trivial fixes (typos, formatting, dead-code removal) are exempt.

**Rationale**: Bugs that escape once are likely to recur. A dedicated
regression test proves the fix works and prevents silent reintroduction
during future refactors.

## Code Quality Constraints

- Code MUST solve only the stated problem; speculative features are forbidden.
- Changes MUST be surgical — adjacent code MUST NOT be reformatted,
  restructured, or "improved" unless directly required by the task.
- Dead code created by your changes MUST be removed; pre-existing dead code
  MAY be mentioned but MUST NOT be deleted unless explicitly requested.
- Existing style MUST be preserved even when personal preference differs.

## Execution Workflow

- Every task MUST begin with explicit assumptions stated to the user.
- Every task MUST have verifiable success criteria before implementation starts.
- Multi-step plans MUST include intermediate verification checkpoints.
- If ambiguity arises during implementation, stop and seek clarification
  rather than guessing.

## Governance

This constitution supersedes all other development practices and style guides
for this project.

### Amendment Procedure

- Any change to these principles requires a documented rationale and a version bump.
- MINOR version bump for added principles or materially expanded guidance.
- MAJOR version bump for backward-incompatible principle removals or redefinitions.
- PATCH version bump for clarifications, wording improvements, or typo fixes.

### Compliance Review

- All pull requests and code reviews MUST verify compliance with these principles.
- Complexity MUST be justified against Principle II (Simplicity First).
- Scope discipline MUST be checked against Principle III (Surgical Changes).
- Every major bug fix MUST include a regression test per Principle V.
- Use `CLAUDE.md` at the repository root for runtime development guidance.

**Version**: 1.1.0 | **Ratified**: TODO(RATIFICATION_DATE): Set when first ratified by project owner | **Last Amended**: 2026-05-03
