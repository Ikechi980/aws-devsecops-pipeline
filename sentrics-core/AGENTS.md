# AGENTS.md

This monorepo contains multiple local projects. Treat the guidance in this file as the default engineering standard for all of them unless a project has stricter local rules.

## Environment Variables

- Use `.env` files for local development.
- Do not rely on implicit defaults for required environment variables.
- Prefer failing immediately at startup if a required environment variable is missing.

### Local Dev Script Policy

- The local development environment is intentionally static.
- Do not add environment-variable overrides for dev orchestration topology in scripts.
- Do not add hidden defaults for dev script configuration.
- If local ports, endpoints, queue names, or similar topology values must change, update the repo-tracked script or infra configuration directly.

## README and High-Level Documentation

- Keep `README.md` concise and focused on:
  - Helping developers get oriented and productive quickly.
  - Providing descriptive deployment context for DevOps without prescribing a single deployment strategy.
- Keep prose clear and easy to read. Avoid clipped language.

## Code

- Prioritize code that is easy to read and maintain.
- Prefer idiomatic, functional Rust when it improves clarity.
- If idiomatic style conflicts with readability, prioritize readability.
- Use Rust types to express domain meaning and enforce invariants.
- Reserve `main()` for initialization, wiring, and startup orchestration.

### Comments

- Use comments sparingly.
- Use comments to:
  - Delineate large sections of related logic.
  - Explain why a non-obvious decision exists.
  - Provide context that is not apparent from the code itself.
- Do not add comments that restate what code already says.
- In tests, comments may describe setup and expectations when not obvious.
- Keep comments short, clear, and readable.

### Linting

- Run `cargo clippy --all-targets` frequently and address warnings.
- Do not use `#![allow(...)]` in production code.

## Tests

- Prefer integration tests when feasible.
- Use unit tests to cover behavior that cannot be simulated locally.
- It is acceptable for integration tests to require the local dev environment and running application.
- If some tests cannot run in parallel, put those cases in one test function to enforce serialization.
- Run tests for each individual project with:
  - `./scripts/dev.sh test`

## Dependency Management

- Prefer current stable dependency versions when possible.
- Use primary documentation and best-practice guidance when adopting dependencies.
- In `Cargo.toml`, use non-breaking version specs:
  - For `1.x.y` and above, pin at major version only (for example, `"1"`).
  - For `0.x.y`, pin at major+minor only (for example, `"0.x"`).
