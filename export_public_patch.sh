#!/bin/bash

# Check if destination path is provided
if [ $# -ne 1 ]; then
    echo "Error: Destination path is required"
    echo "Usage: $0 [DESTINATION_PATH]"
    exit 1
fi

# Configuration
DESTINATION_PATH="$1"  # Use the provided path
REFERENCE_COMMIT_FILE="reference.commit"
SCRIPT_NAME=$(basename "$0")

# Validate destination path
if [ -e "$DESTINATION_PATH" ] && [ ! -d "$DESTINATION_PATH" ]; then
    echo "Error: Destination path exists but is not a directory"
    exit 1
fi

# Ensure we're in a git repository
if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "Error: Not a git repository"
    exit 1
fi

# Create a temporary file listing
TEMP_FILE_LIST=$(mktemp)
git ls-files > "$TEMP_FILE_LIST"
# Remove this script from the list
sed -i "\:^${SCRIPT_NAME}$:d" "$TEMP_FILE_LIST"

# Ensure destination directory exists
mkdir -p "$DESTINATION_PATH"

# Get current commit hash
CURRENT_COMMIT=$(git rev-parse HEAD)

# Use rsync with --files-from to copy only git-tracked files
rsync -av \
    --files-from="$TEMP_FILE_LIST" \
    ./ "$DESTINATION_PATH/"

# Clean up temp file
rm "$TEMP_FILE_LIST"

# Create reference.commit file with current commit hash
echo "$CURRENT_COMMIT" > "$DESTINATION_PATH/$REFERENCE_COMMIT_FILE"

echo "Export completed successfully!"
echo "Files copied to: $DESTINATION_PATH"
echo "Reference commit saved to: $DESTINATION_PATH/$REFERENCE_COMMIT_FILE"

