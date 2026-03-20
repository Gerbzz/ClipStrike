# Workflow: ClipStrike

## Development Protocol
1.  **Specification:** Define requirements in `spec.md`.
2.  **Planning:** Create a step-by-step `plan.md`.
3.  **Implementation:** Build features task-by-task.
4.  **Verification:** Every feature must be verified with automated tests (Playwright/WASM and Rust unit tests).

## Testing Strategy
- **Rust Unit Tests:** For core logic, timeline math, and data structures.
- **Playwright / WASM:** For E2E UI verification and engine hooks.
- **Manual Verification:** Required for visual fidelity and audio synchronization.

## Commit Strategy
- Atomic commits per task.
- Clear, descriptive messages explaining *why* a change was made.
- Never commit broken builds.
