---
allowed-tools: Bash(git diff:*), Bash(git status:*), Bash(git log:*), Bash(cargo test:*), Bash(cargo build:*), Bash(cargo clippy:*)
description: Review code changes for alignment with project standards (tests, CLAUDE.md, architecture, cleanup)
---

# Align Command

Review the current branch's changes against main for alignment with project standards.

## Context

- Current branch: ${{CURRENT_BRANCH}}
- Main branch diff: ${{GIT_DIFF_STAT}}

## Workflow

**Agent assumptions (applies to all agents):**
- All tools are functional. Do not test tools or make exploratory calls.
- Only call a tool if required. Every tool call should have a clear purpose.
- Focus on HIGH SIGNAL issues only. False positives erode trust.

### Phase 1: Precondition Check

Launch a haiku agent to check:
- Are there any changes vs main? (if not, stop)
- Is this branch trivial (single typo fix, config change)? Note but continue.

If no changes, report "No changes to align" and stop.

### Phase 2: Context Gathering

Launch a haiku agent to:
1. Read the CLAUDE.md file at the repo root
2. List all files changed in this branch vs main (`git diff main...HEAD --name-only`)
3. Note any directory-specific conventions from CLAUDE.md

### Phase 3: Change Summary

Launch a sonnet agent to:
1. View the full diff (`git diff main...HEAD`)
2. Summarize what changed at a high level
3. Categorize: new feature, bug fix, refactor, enhancement, etc.

Return the summary for use in Phase 4 prompts.

### Phase 4: Parallel Review

Launch 4 agents in parallel. Each agent receives:
- The change summary from Phase 3
- The relevant CLAUDE.md sections
- The list of changed files

Each agent returns issues with confidence scores (0-100). Only report issues with confidence >= 80.

**Agent 1: Test Coverage Reviewer (sonnet)**
```
Review test coverage for the changes. Consider:
- CLI commands need integration tests (per CLAUDE.md)
- TUI code is manual testing only (per CLAUDE.md)
- Connectors need unit tests for parsing/caching
- Core utilities need unit tests

Check:
1. Do new public functions have tests?
2. Do modified functions need test updates?
3. Are there missing edge cases?

Return: List of files/functions needing tests with confidence scores.
```

**Agent 2: CLAUDE.md Compliance Reviewer (sonnet)**
```
Audit changes against CLAUDE.md conventions:
- Error handling: anyhow::Result in commands, GwtError for user-facing
- Module organization: business logic in core/connectors, not commands/tui
- Async patterns: never .await in TUI event loop, use channels
- Anti-patterns: no global state, use existing parsers/cache helpers

For each violation, quote the specific CLAUDE.md rule.

Return: List of violations with file:line, rule quote, confidence.
```

**Agent 3: Architecture Reviewer (opus)**
```
Assess architectural soundness:
- Modularity: tight coupling? appropriate abstractions?
- Code reuse: duplicating existing utilities?
- Design patterns: follows connector/command patterns?
- Integration: clean boundaries between TUI/CLI/connectors?
- Scalability: would this cause problems at 10x scale?

Focus on structural issues, not style.

Return: Architecture observations with severity (Critical/Important/Minor).
```

**Agent 4: Code Cleanup Reviewer (sonnet)**
```
Find cleanup opportunities:
- Code duplication that should be extracted
- Dead code: unused imports, unreachable branches
- Unnecessary complexity: overcomplicated logic
- Rust-specific: unnecessary clones, missing ? operator

Focus on clearly fixable items, not style preferences.

Return: Cleanup items with file:line and effort estimate.
```

### Phase 5: Issue Validation

For issues flagged with confidence < 95 by Architecture or Cleanup agents:
- Launch parallel opus subagents to validate
- Each validator gets: issue description, relevant code context
- Validator confirms (true positive) or rejects (false positive)

Filter out rejected issues.

### Phase 6: Summary and Actions

Compile validated issues into structured summary:

```
## Alignment Review Summary

**Branch**: [branch name]
**Changes**: X files, +Y/-Z lines
**Type**: [new feature/bug fix/refactor/etc.]

### Test Coverage
- [ ] Issue 1
- [x] Already covered

### CLAUDE.md Compliance
- [ ] Issue 1 (quotes rule)

### Architecture
- [ ] Issue 1 (severity)

### Code Cleanup
- [ ] Issue 1

### Recommended Actions
1. Priority action 1
2. Priority action 2
```

Then ask: "Would you like me to fix these issues?"

If user agrees:
1. Fix issues in priority order (Critical > Important > Minor)
2. After each category, run `cargo build` to verify
3. Update the checklist as issues are fixed
4. Run `cargo clippy` at the end for final check

If no issues found:
- Report "Changes are well-aligned with project standards"
