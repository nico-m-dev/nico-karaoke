# Security policy

## Reporting a vulnerability

Please do not open a public issue for security problems. Use GitHub's private
vulnerability reporting for this repository:

https://github.com/nico-m-dev/nico-karaoke/security/advisories/new

Include:

- what the problem is and where it is
- how to reproduce it
- the affected version or commit
- the impact, and any suggested fix

I aim to respond within a few days. Please give me time to fix the issue before
publishing details.

## Supported versions

The latest release and the `main` branch are supported. Older releases may not
receive fixes.

## Scope

This policy covers the application code, build scripts and release artifacts.
Problems in third-party dependencies belong upstream, unless this repository
uses them in a way that creates a new issue.

Do not include live credentials in a report. If a secret may have been exposed,
rotate it first.
