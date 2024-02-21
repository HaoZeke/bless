#!/bin/bash

# Org to Markdown Exporter
# Converts an Org-mode file to Markdown format using Emacs in batch mode.
#
# Usage:
#   ./org_to_md.sh <input-org-file> [output-markdown-file]
#
# If the output file is not provided, the script uses the input file's name with a .md extension.
#
# Author: Rohit Goswami (HaoZeke)
# License: MIT
# Website: https://rgoswami.me

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "Usage: $0 <input-org-file> [output-markdown-file]"
    exit 1
fi

# Input Org file
ORG_FILE=$1

# Output Markdown file
MD_FILE=${2:-"${ORG_FILE%.org}.md"}

# Check if the output file name is different from the input file name
if [ "${ORG_FILE%.org}.md" != "$MD_FILE" ]; then
    # Temporary Org file with the final Markdown filename
    TEMP_ORG_FILE="${MD_FILE%.md}.org"

    # Copy original Org file to temporary file
    cp "$ORG_FILE" "$TEMP_ORG_FILE"

    # Use the temporary file for exporting
    EXPORT_FILE="$TEMP_ORG_FILE"
else
    # Use the original file for exporting
    EXPORT_FILE="$ORG_FILE"
fi

# Emacs command to export Org file to Markdown
emacs --batch "$EXPORT_FILE" \
    --eval "(require 'ox-md nil t)" \
    --eval "(org-md-export-to-markdown)" \
    --kill

# Cleanup: remove temporary Org file if it was created
if [ -f "$TEMP_ORG_FILE" ]; then
    rm "$TEMP_ORG_FILE"
fi

# Check if Markdown file was created
if [ -f "$MD_FILE" ]; then
    echo "Exported to $MD_FILE"
else
    echo "Export failed."
fi
