# Update Page Localization Design

Date: 2026-07-26
Status: Approved

## Goal

Make the application update screen follow the existing English and Simplified Chinese language setting.

## Scope

- Add `updates.*` entries to the existing frontend i18n dictionaries.
- Replace every fixed update-screen label, action, status, empty state, warning, and fallback date with `useI18n()` calls.
- Keep dynamic values such as versions, download byte counts, endpoint URLs, and updater error messages unchanged.
- Keep `update.body` unchanged because GitHub Release notes are publisher-supplied content and may already include localized text.

## Verification

- Add a Vitest render test that mounts `UpdatesScreen` with `I18nProvider initialLanguage="zh-CN"` and asserts representative Chinese labels.
- Run `pnpm typecheck` and `pnpm test:run`.
