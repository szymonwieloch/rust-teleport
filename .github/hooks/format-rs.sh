#!/usr/bin/env bash
# PostToolUse hook: auto-format .rs files with cargo fmt after they are written.
set -euo pipefail

# Read hook input from stdin
INPUT=$(cat)

# Extract tool name and file path from the hook input
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')
FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.filePath // empty')

# Only run for file-writing tools targeting .rs files
case "$TOOL_NAME" in
  create_file|replace_string_in_file|insert_edit_into_file)
    if [[ "$FILE_PATH" == *.rs ]]; then
      # Find the nearest Cargo.toml to determine the crate root
      CRATE_DIR=$(dirname "$FILE_PATH")
      while [[ "$CRATE_DIR" != "/" ]] && [[ ! -f "$CRATE_DIR/Cargo.toml" ]]; do
        CRATE_DIR=$(dirname "$CRATE_DIR")
      done

      if [[ -f "$CRATE_DIR/Cargo.toml" ]]; then
        cd "$CRATE_DIR"
        if command -v cargo &>/dev/null && cargo fmt -- "$FILE_PATH" 2>/dev/null; then
          echo "✓ Formatted: $FILE_PATH" >&2
        fi
      fi
    fi
    ;;
esac

# Always allow continuation
exit 0
