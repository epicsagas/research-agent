# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

**Do not report security vulnerabilities through public GitHub issues.**

Instead, use [GitHub Private Vulnerability Reporting](https://github.com/epicsagas/research-agent/security/advisories/new).

### Response SLA

| Stage | Target |
|-------|--------|
| Acknowledgement | 48 hours |
| Initial assessment | 5 business days |
| Patch / advisory | 90 days or coordinated disclosure |

### What to include

- Description of the vulnerability
- Steps to reproduce
- Affected versions
- Potential impact

### Scope

- SQL injection in query/index commands
- Path traversal in PDF ingestion
- API key leakage in logs or error messages
- Supply chain concerns in CI/CD

### Out of scope

- Denial of service via abnormally large paper databases
- Issues in dependencies not triggered by this project's code
