# Security Policy

## Dependency Auditing
To check for vulnerable cryptographic dependencies, we use `cargo-deny` in our CI pipeline and `cargo audit` as a pre-push hook.

### Checked Databases
The automated tooling checks the following advisory databases:
- **RustSec Advisory Database** (https://github.com/rustsec/advisory-db)

### Update Policy for Vulnerabilities
- Any `RUSTSEC` advisory with a severity of HIGH or CRITICAL must be updated **immediately** (0 days).
- Advisories with a severity of MEDIUM or LOW are acceptable for up to **7 days** before the dependency must be updated or replaced.
- If an update is not immediately available, the vulnerability must be explicitly acknowledged and a mitigation strategy must be documented.
