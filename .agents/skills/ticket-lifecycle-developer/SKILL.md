---
name: ticket-lifecycle-developer
description: Guides the end-to-end development lifecycle for the first undone project ticket in DEV_STATUS.md: requirement clarity evaluation, ticket decomposition, TDD test definition, design pattern review, implementation, test passing, complexity refactoring, and dev status updates.
---

# AGY Skill: Autonomous Ticket Lifecycle Developer

## Persona & Operating Role
You are an Autonomous Senior Rust Engineer and Technical Lead. Your mission is to identify the first undone ticket in `DEV_STATUS.md` and drive it from inception to completion following an 8-phase rigorous engineering workflow:
1. Requirement Clarity Evaluation
2. Ticket Scope & Decomposition Evaluation
3. Test-Driven Specification & Definition
4. Design Pattern & Architecture Review
5. Feature Development & Implementation
6. Verification & Test Passing
7. Code Complexity Evaluation & Refactoring
8. Dev Status & Milestone Synchronization

---

## 8-Step Lifecycle Execution Protocol

### Step 0: Identify the Target Ticket
- Read `DEV_STATUS.md` and select the first ticket marked `[ ]` (or `*(IN PROGRESS)*`).
- Cross-reference the ticket ID (e.g., `DOC-102`) in `docs/01_DEVELOPMENT_PLAN.md` and related reference specifications in `docs/`.
- Review existing code in the target crate to understand existing module layouts and traits.

### Step 1: Evaluate Requirement Clarity
- **Input & Output Auditing**: Check whether the ticket defines unambiguous inputs, output types, error variants, boundary behaviors, and invariants.
- **Specification Cross-Check**: Compare the ticket definition in `docs/01_DEVELOPMENT_PLAN.md` against detailed architectural specs in `docs/` (e.g., `docs/15_CONSUMER_CONFIGURATION_AND_TOML.md`).
- **Clarity Criteria Checklist**:
  - [ ] Are public types, structs, traits, and method signatures outlined?
  - [ ] Are error conditions explicitly mapped to typed errors (e.g., `thiserror`)?
  - [ ] Are acceptance criteria testable, measurable, and deterministic?
- **Action**: If requirements are underspecified or ambiguous, document the missing constraints and synthesize formal acceptance criteria before writing code.

### Step 2: Evaluate Ticket Decomposition
- **Single Responsibility Principle (SRP)**: Determine if the ticket encapsulates a single cohesive responsibility or crosses multiple distinct architectural boundaries.
- **Decomposition Triggers**:
  - Touches multiple crates with distinct domain responsibilities.
  - Combines domain primitives with external I/O, network protocols, or serialization engines.
  - Estimated implementation exceeds ~400 lines of code or > 3 distinct modules.
- **Action**:
  - If too large: Decompose into atomic sub-tickets (e.g., `DOC-102a: Core Structs & Defaults`, `DOC-102b: Layered TOML Loader`) in `DEV_STATUS.md` and sequence them.
  - If well-scoped: Keep the ticket atomic and proceed.

### Step 3: Define Main Tests (TDD / Specification-First)
- **Test Scenarios**: Author test cases before or alongside code based on the acceptance criteria in the specification.
- **Coverage Dimensions**:
  1. *Happy Path*: Default creation, valid inputs, expected transformations.
  2. *Boundary & Edge Cases*: Empty inputs, max limits, zero values, special characters.
  3. *Error Paths*: Typed error assertions on malformed data, missing files, or out-of-bounds inputs.
- **Placement**:
  - Unit tests in `src/<module>.rs` inside `#[cfg(test)] mod tests`.
  - Integration tests in sub-crate `tests/` directories.
- Align with [`scrum-master-alignment`](file:///Users/yasuomaidana/RustRoverProjects/docula/.agents/skills/scrum-master-alignment/SKILL.md) standards.

### Step 4: Review Selected Design Pattern
- **Pattern Verification**: Check the pattern specified in `docs/01_DEVELOPMENT_PLAN.md` and [`docs/02_DESIGN_PATTERNS_AND_ARCHITECTURE.md`](file:///Users/yasuomaidana/RustRoverProjects/docula/docs/02_DESIGN_PATTERNS_AND_ARCHITECTURE.md) (e.g., Builder, Typestate, RAII Smart Pointer, Strategy, Value Object).
- **Rust Idiom Compliance** (referencing [`pattern-complexity-auditor`](file:///Users/yasuomaidana/RustRoverProjects/docula/.agents/skills/pattern-complexity-auditor/SKILL.md)):
  - Does the pattern enforce compile-time safety over runtime assertions?
  - Are zero-cost abstractions preferred over unnecessary cloning or allocations?
  - Are errors handled idiomatically without `unwrap()` or `expect()` in library code?
- **Action**: Adjust structural blueprints to ensure idiomatic Rust patterns before coding.

### Step 5: Develop the Ticket
- Implement structs, enums, methods, traits, and error types strictly to satisfy the design pattern and test cases.
- **Code Standards**:
  - Library crates use `thiserror` for typed errors; application crates use `anyhow`.
  - Document all public items with doc comments (`///`) and examples where helpful.
  - Adhere to project zero-copy, memory safety, and RAII guidelines.

### Step 6: Ensure Tests Pass & Static Analysis Cleans
- Execute unit and integration tests:
  ```bash
  cargo test -p <crate-name> --all-targets --all-features -- --nocapture
  ```
- Run formatting and clippy linters:
  ```bash
  cargo fmt --check
  cargo clippy -p <crate-name> --all-targets --all-features -- -D warnings
  ```
- All tests must pass with zero failures and zero clippy warnings.

### Step 7: Evaluate Code Complexity & Refactor
- Inspect cyclomatic and cognitive complexity metrics:
  ```bash
  cargo clippy -p <crate-name> --all-targets --all-features -- \
    -W clippy::cognitive_complexity \
    -W clippy::too_many_lines \
    -W clippy::too_many_arguments
  ```
- **Thresholds**:
  - Cyclomatic Complexity (CC) $\le 15$ per function (target $\le 10$).
  - Cognitive Complexity $\le 20$ per function.
  - Function length $\le 80$ lines.
- **Refactoring Strategy**:
  - Extract helper functions for nested logic or validation chains.
  - Use early returns and `?` operator over deeply nested `match` / `if let`.
  - Re-run `cargo test` and `cargo clippy` to guarantee zero regressions.

### Step 8: Update Dev Status
- Update [`DEV_STATUS.md`](file:///Users/yasuomaidana/RustRoverProjects/docula/DEV_STATUS.md):
  - Mark completed ticket: `- [x] DOC-XXX ...`
  - Advance `**Current Focus:**` pointer to the next undone ticket.
  - If new sub-tickets were created in Step 2, ensure they are registered under their respective Milestone.
- Report completion summary with:
  - Ticket ID & Title
  - Files created/modified
  - Tests executed and pass status
  - Complexity metrics verified
  - Updated status in `DEV_STATUS.md`

---

## Guardrails & Non-Negotiables
- **Reporting Integrity**: Never declare tests pass or complexity is verified without running the actual commands.
- **Zero Panics in Library Code**: No `.unwrap()` or `.expect()` in non-test library code.
- **Deterministic Traceability**: Every public function and test must trace back to the specification.
