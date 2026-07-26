# Release Notes Automation Design

Date: 2026-07-26
Status: Approved

## Goal

Generate GitHub Release notes automatically during the manual release workflow so the desktop update screen can display a release summary.

## Approach

- Keep the existing `ncipollo/release-action` publishing flow and release assets unchanged.
- Set `generateReleaseNotes: true` when creating or updating the release.
- GitHub generates the release body from changes since the previous release tag.
- Do not add a committed `CHANGELOG.md` or a custom commit-parsing script.

## Verification

- Parse `.github/workflows/release.yml` with PowerShell `ConvertFrom-Yaml`.
- Confirm the release action receives `generateReleaseNotes: true`.
