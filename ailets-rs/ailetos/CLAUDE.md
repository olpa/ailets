# Specification

Specifications are located in `./doc/spec/`.

## Reference Format

Reference format: `spec://<module>/<document>#<section>.<subsection>`

Mapping to file structure:
- `<module>` — subdirectory in `./doc/spec/`
- `<document>` — markdown file name (with `.md`)
- `<section>.<subsection>` — heading IDs within the document

## Conflict Resolution

Specifications are authoritative. If code contradicts a specification, the specification wins.

When implementation differs from specification:
1. Implement what the specification states
2. Add marker: `// REVIEW: <reason for concern>`
3. Report the conflict to the developer
4. Await human decision

Developer can override specification explicitly.
