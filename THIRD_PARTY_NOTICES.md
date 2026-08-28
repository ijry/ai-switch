# Third-Party Notices

## codeg MCP and Skills implementation

Parts of the MCP and Skills settings implementation are derived from and adapted from:

- Project: [xintaofei/codeg](https://github.com/xintaofei/codeg)
- License: Apache License 2.0
- Source snapshot: `df7a872de44546277e4c49cfe9d173c631161dc6`

AI Switch reorganizes the implementation into independent MCP and Skills modules,
adapts error and transport boundaries, and adds support for this repository's
Tauri/Web command layer. The original Apache-2.0 notices are retained in the
derived source files. See `LICENSES/Apache-2.0.txt`.

## codeg bilingual release notes rendering

The bilingual release-notes splitting and the markdown notes panel are derived
from and adapted from:

- Project: [xintaofei/codeg](https://github.com/xintaofei/codeg)
- License: Apache License 2.0
- Source files: `src/lib/release-notes.ts`, `src/components/settings/release-notes.tsx`

AI Switch ports the separator detection and language scoring to this
repository's own i18n context and UnoCSS typography. The Apache-2.0 notices are
retained in the derived source files. See `LICENSES/Apache-2.0.txt`.
